import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Download, FileText, Paperclip, Trash2 } from "lucide-react";
import { useState } from "react";
import { downloadAttachment } from "@/api/download";
import { errorMessage } from "@/api/errors";
import { Field } from "@/components/Field";
import { QueryState } from "@/components/QueryState";
import { Alert } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { saveFile } from "@/lib/save-file";
import { formatSize } from "./format-size";
import {
  ACCEPTED_FILES,
  attachmentsQuery,
  deleteAttachment,
  MAX_ATTACHMENT_BYTES,
  uploadAttachment,
  type Attachment,
  type AttachmentTarget,
} from "./queries";

interface AttachmentsPanelProps {
  target: AttachmentTarget;
  /** Defaults to "Attachments"; a group with several splits names the split. */
  title?: string;
}

/** The files of one split or bill: list, download, delete and upload. */
export function AttachmentsPanel({ target, title = "Attachments" }: AttachmentsPanelProps) {
  const attachments = useQuery(attachmentsQuery(target));
  const files = attachments.data ?? [];
  return (
    <Card>
      <CardHeader>
        <CardTitle className="flex items-center gap-2">
          <Paperclip className="size-4" /> {title}
        </CardTitle>
        <CardDescription>PDF, images, text, CSV or XML, up to 20 MB each.</CardDescription>
      </CardHeader>
      <CardContent className="flex flex-col gap-4">
        <QueryState isPending={attachments.isPending} error={attachments.error} />
        {attachments.data && files.length === 0 ? (
          <p className="text-sm text-muted-foreground">No files yet.</p>
        ) : null}
        {files.length > 0 ? (
          <ul className="flex flex-col divide-y rounded-md border">
            {files.map((file) => (
              <AttachmentRow key={file.id} file={file} target={target} />
            ))}
          </ul>
        ) : null}
        <UploadForm target={target} />
      </CardContent>
    </Card>
  );
}

function AttachmentRow({ file, target }: { file: Attachment; target: AttachmentTarget }) {
  const queryClient = useQueryClient();
  const [confirming, setConfirming] = useState(false);
  const name = file.title ?? file.filename;
  const download = useMutation({
    mutationFn: () => downloadAttachment(file.id, file.filename),
    onSuccess: ({ blob, filename }) => saveFile(blob, filename),
  });
  const remove = useMutation({
    mutationFn: () => deleteAttachment(file.id),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: attachmentsQuery(target).queryKey }),
  });
  const error = download.error ?? remove.error;
  return (
    <li className="flex flex-col gap-2 p-3">
      <div className="flex items-center gap-3">
        <FileText className="size-5 shrink-0 text-muted-foreground" aria-hidden />
        <div className="min-w-0 flex-1">
          <p className="truncate text-sm font-medium">{name}</p>
          <p className="truncate text-xs text-muted-foreground">
            {file.title ? `${file.filename} · ` : ""}
            {formatSize(file.size_bytes)}
          </p>
        </div>
        <Button
          size="sm"
          variant="ghost"
          aria-label={`Download ${name}`}
          disabled={download.isPending}
          onClick={() => download.mutate()}
        >
          <Download />
        </Button>
        <Button
          size="sm"
          variant={confirming ? "destructive" : "ghost"}
          aria-label={confirming ? `Really delete ${name}?` : `Delete ${name}`}
          disabled={remove.isPending}
          onClick={() => (confirming ? remove.mutate() : setConfirming(true))}
        >
          <Trash2 /> {confirming ? "Really delete?" : null}
        </Button>
      </div>
      {error ? <Alert>{errorMessage(error)}</Alert> : null}
    </li>
  );
}

function UploadForm({ target }: { target: AttachmentTarget }) {
  const queryClient = useQueryClient();
  const [file, setFile] = useState<File | null>(null);
  const [title, setTitle] = useState("");
  // A new key empties the file input after an upload (it cannot be set).
  const [inputKey, setInputKey] = useState(0);
  const idPrefix = `attachment-${target.type}-${target.id}`;
  const tooLarge = file !== null && file.size > MAX_ATTACHMENT_BYTES;
  const upload = useMutation({
    mutationFn: (chosen: File) => uploadAttachment(target, chosen, title),
    onSuccess: async () => {
      setFile(null);
      setTitle("");
      setInputKey((key) => key + 1);
      await queryClient.invalidateQueries({ queryKey: attachmentsQuery(target).queryKey });
    },
  });
  return (
    <form
      className="flex flex-col gap-3 sm:flex-row sm:items-end"
      onSubmit={(event) => {
        event.preventDefault();
        if (file && !tooLarge) upload.mutate(file);
      }}
    >
      <Field
        id={`${idPrefix}-file`}
        label="File"
        className="sm:flex-1"
        error={tooLarge ? "This file is larger than 20 MB." : undefined}
      >
        <Input
          key={inputKey}
          id={`${idPrefix}-file`}
          type="file"
          accept={ACCEPTED_FILES}
          aria-invalid={tooLarge}
          onChange={(event) => setFile(event.target.files?.[0] ?? null)}
        />
      </Field>
      <Field id={`${idPrefix}-title`} label="Title (optional)" className="sm:flex-1">
        <Input
          id={`${idPrefix}-title`}
          value={title}
          onChange={(event) => setTitle(event.target.value)}
        />
      </Field>
      <Button type="submit" disabled={!file || tooLarge || upload.isPending}>
        {upload.isPending ? "Uploading…" : "Upload"}
      </Button>
      {upload.error ? <Alert className="sm:basis-full">{errorMessage(upload.error)}</Alert> : null}
    </form>
  );
}
