//! The snapshot zip: `saldo.db` plus `attachments/<sha256>` files. Writing
//! and reading are blocking file work; callers run them off the async runtime.

use std::fs::File;
use std::io::{Seek, Write};
use std::path::{Path, PathBuf};

use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use crate::attachments::storage::is_sha256;
use crate::error::AppError;

/// The database's name inside the zip.
pub const DATABASE_ENTRY: &str = "saldo.db";

/// Folders under the data dir whose files go into a snapshot. Only files
/// named by their SHA-256 are taken, so half-written `.partial` files stay out.
const FILE_FOLDERS: [&str; 1] = ["attachments"];

/// Writes the zip to `out`. The database is compressed; the files are stored
/// as they are (mostly PDFs and photos, which do not compress).
pub fn write(database: &Path, data_dir: &Path, out: File) -> Result<File, AppError> {
    let mut zip = ZipWriter::new(out);
    let compressed = options(CompressionMethod::Deflated);
    add_file(&mut zip, DATABASE_ENTRY, database, compressed)?;
    for folder in FILE_FOLDERS {
        for (name, path) in stored_files(&data_dir.join(folder))? {
            add_file(
                &mut zip,
                &format!("{folder}/{name}"),
                &path,
                options(CompressionMethod::Stored),
            )?;
        }
    }
    zip.finish().map_err(zip_error)
}

/// `large_file` allows entries over 4 GiB (ZIP64).
fn options(method: CompressionMethod) -> SimpleFileOptions {
    SimpleFileOptions::default()
        .compression_method(method)
        .large_file(true)
}

fn add_file<W: Write + Seek>(
    zip: &mut ZipWriter<W>,
    name: &str,
    path: &Path,
    options: SimpleFileOptions,
) -> Result<(), AppError> {
    zip.start_file(name, options).map_err(zip_error)?;
    let mut file = File::open(path).map_err(io_error)?;
    std::io::copy(&mut file, zip).map_err(io_error)?;
    Ok(())
}

/// The folder's SHA-256-named files, sorted; no folder means no files.
fn stored_files(folder: &Path) -> Result<Vec<(String, PathBuf)>, AppError> {
    let entries = match std::fs::read_dir(folder) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(io_error(error)),
    };
    let mut files = Vec::new();
    for entry in entries {
        let entry = entry.map_err(io_error)?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if is_sha256(&name) && entry.file_type().map_err(io_error)?.is_file() {
            files.push((name, entry.path()));
        }
    }
    files.sort();
    Ok(files)
}

/// What a restore wrote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Restored {
    /// Files written under the data dir (attachments).
    pub files: usize,
}

/// Unpacks `archive`: the files under `data_dir`, then the database to
/// `database`, which must not exist. Every entry is checked before anything
/// is written, so a zip with an unexpected name (`../x`, an absolute path, an
/// unknown folder) writes nothing. The database comes last and is never
/// written over, so a restore that failed on a file can simply run again.
pub fn restore(archive: &Path, database: &Path, data_dir: &Path) -> Result<Restored, AppError> {
    if database.exists() {
        return Err(database_exists(database));
    }
    let mut zip = ZipArchive::new(File::open(archive).map_err(io_error)?).map_err(zip_error)?;
    let names = entry_names(&mut zip)?;
    check_entries(&names)?;
    let mut files = 0;
    let mut database_index = 0;
    for (index, name) in names.iter().enumerate() {
        if name == DATABASE_ENTRY {
            database_index = index;
            continue;
        }
        let mut entry = zip.by_index(index).map_err(zip_error)?;
        // A file already there has the same SHA-256 name, so the same bytes.
        write_new(&data_dir.join(name), &mut entry)?;
        files += 1;
    }
    let mut entry = zip.by_index(database_index).map_err(zip_error)?;
    if !write_new(database, &mut entry)? {
        return Err(database_exists(database));
    }
    Ok(Restored { files })
}

fn database_exists(database: &Path) -> AppError {
    AppError::Conflict(format!(
        "{} already exists; restore only into a fresh Saldo",
        database.display()
    ))
}

/// Every entry's name, in index order.
fn entry_names(zip: &mut ZipArchive<File>) -> Result<Vec<String>, AppError> {
    (0..zip.len())
        .map(|index| {
            let entry = zip.by_index(index).map_err(zip_error)?;
            let name = entry.name().map_err(zip_error)?;
            Ok(name.into_owned())
        })
        .collect()
}

/// Exactly one `saldo.db`, otherwise only `<known folder>/<sha256>`, and no
/// name twice.
fn check_entries(names: &[String]) -> Result<(), AppError> {
    let mut unique = names.to_vec();
    unique.sort();
    unique.dedup();
    if unique.len() != names.len() {
        return Err(AppError::invalid("archive", "holds an entry twice"));
    }
    let databases = names.iter().filter(|n| *n == DATABASE_ENTRY).count();
    if databases != 1 {
        return Err(AppError::invalid(
            "archive",
            format!("must hold exactly one {DATABASE_ENTRY}, found {databases}"),
        ));
    }
    let unexpected = names.iter().find(|name| {
        *name != DATABASE_ENTRY
            && !name
                .split_once('/')
                .is_some_and(|(folder, file)| FILE_FOLDERS.contains(&folder) && is_sha256(file))
    });
    match unexpected {
        Some(name) => Err(AppError::invalid(
            "archive",
            format!("unexpected entry {name:?}; is this a Saldo snapshot?"),
        )),
        None => Ok(()),
    }
}

/// Writes to `<target>.partial`, then links it to `target` unless `target`
/// exists (`false`). A failed restore never leaves a half-written file under
/// its real name, and a database that appeared meanwhile (a server started
/// during the restore) is never replaced, as a rename would.
fn write_new(target: &Path, source: &mut impl std::io::Read) -> Result<bool, AppError> {
    if let Some(folder) = target.parent() {
        std::fs::create_dir_all(folder).map_err(io_error)?;
    }
    let temporary = target.with_extension("partial");
    let mut file = File::create(&temporary).map_err(io_error)?;
    std::io::copy(source, &mut file).map_err(io_error)?;
    file.sync_all().map_err(io_error)?;
    let linked = match std::fs::hard_link(&temporary, target) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => Ok(false),
        Err(error) => Err(io_error(error)),
    };
    std::fs::remove_file(&temporary).map_err(io_error)?;
    linked
}

fn io_error(error: std::io::Error) -> AppError {
    AppError::Internal(format!("snapshot: {error}"))
}

fn zip_error(error: zip::result::ZipError) -> AppError {
    AppError::invalid("archive", format!("not a readable zip: {error}"))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    const SHA: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    fn temp_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("saldo-archive-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn zip_with(dir: &Path, entries: &[(&str, &[u8])]) -> PathBuf {
        let path = dir.join("test.zip");
        let mut zip = ZipWriter::new(File::create(&path).unwrap());
        for (name, bytes) in entries {
            zip.start_file(*name, SimpleFileOptions::default()).unwrap();
            zip.write_all(bytes).unwrap();
        }
        zip.finish().unwrap();
        path
    }

    #[test]
    fn round_trips_the_database_and_stored_files_only() {
        let source = temp_dir();
        let database = source.join("copy.db");
        std::fs::write(&database, b"database bytes").unwrap();
        let attachments = source.join("attachments");
        std::fs::create_dir_all(&attachments).unwrap();
        std::fs::write(attachments.join(SHA), b"a receipt").unwrap();
        std::fs::write(attachments.join(format!("{SHA}.partial")), b"half").unwrap();
        let archive = source.join("snapshot.zip");

        write(&database, &source, File::create(&archive).unwrap()).unwrap();
        let target = temp_dir();
        let restored = restore(&archive, &target.join("db/saldo.db"), &target).unwrap();

        assert_eq!(restored, Restored { files: 1 });
        assert_eq!(
            std::fs::read(target.join("db/saldo.db")).unwrap(),
            b"database bytes"
        );
        assert_eq!(
            std::fs::read(target.join("attachments").join(SHA)).unwrap(),
            b"a receipt"
        );
        assert!(
            !target
                .join("attachments")
                .join(format!("{SHA}.partial"))
                .exists()
        );
    }

    #[test]
    fn writes_a_zip_without_an_attachments_folder() {
        let source = temp_dir();
        let database = source.join("copy.db");
        std::fs::write(&database, b"db").unwrap();
        let archive = source.join("snapshot.zip");

        write(&database, &source, File::create(&archive).unwrap()).unwrap();

        let mut zip = ZipArchive::new(File::open(&archive).unwrap()).unwrap();
        assert_eq!(entry_names(&mut zip).unwrap(), vec![DATABASE_ENTRY]);
    }

    #[test]
    fn never_overwrites_an_existing_database() {
        let dir = temp_dir();
        let archive = zip_with(&dir, &[(DATABASE_ENTRY, b"new")]);
        let database = dir.join("saldo.db");
        std::fs::write(&database, b"old").unwrap();

        let error = restore(&archive, &database, &dir).unwrap_err();

        assert!(matches!(error, AppError::Conflict(_)));
        assert_eq!(std::fs::read(&database).unwrap(), b"old");
    }

    #[test]
    fn refuses_unexpected_entries_before_writing_anything() {
        let unsafe_names = [
            "../evil".to_string(),
            "/etc/passwd".to_string(),
            "attachments/../../evil".to_string(),
            "attachments/not-a-hash".to_string(),
            format!("other/{SHA}"),
        ];
        for name in unsafe_names {
            let dir = temp_dir();
            let archive = zip_with(&dir, &[(DATABASE_ENTRY, b"db"), (&name, b"x")]);
            let database = dir.join("out/saldo.db");

            let error = restore(&archive, &database, &dir.join("out")).unwrap_err();

            assert!(matches!(error, AppError::Invalid { .. }), "{name}");
            assert!(!database.exists(), "{name}");
        }
    }

    #[test]
    fn runs_again_over_files_a_failed_restore_left() {
        let dir = temp_dir();
        let archive = zip_with(
            &dir,
            &[
                (&format!("attachments/{SHA}"), b"x"),
                (DATABASE_ENTRY, b"db"),
            ],
        );
        let out = dir.join("out");
        std::fs::create_dir_all(out.join("attachments")).unwrap();
        std::fs::write(out.join("attachments").join(SHA), b"x").unwrap();

        let restored = restore(&archive, &out.join("saldo.db"), &out).unwrap();

        assert_eq!(restored, Restored { files: 1 });
        assert_eq!(std::fs::read(out.join("saldo.db")).unwrap(), b"db");
        assert!(!out.join("saldo.partial").exists());
    }

    #[test]
    fn needs_exactly_one_database() {
        let dir = temp_dir();
        let archive = zip_with(&dir, &[(&format!("attachments/{SHA}"), b"x")]);

        let error = restore(&archive, &dir.join("saldo.db"), &dir).unwrap_err();

        assert!(matches!(error, AppError::Invalid { .. }));
    }

    #[test]
    fn refuses_a_file_that_is_not_a_zip() {
        let dir = temp_dir();
        let archive = dir.join("snapshot.zip");
        std::fs::write(&archive, b"not a zip").unwrap();

        let error = restore(&archive, &dir.join("saldo.db"), &dir).unwrap_err();

        assert!(matches!(error, AppError::Invalid { .. }));
    }
}
