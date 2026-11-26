import { useState } from "react";
import { Button } from "@/components/ui/button";
import { Textarea } from "@/components/ui/input";
import { NotesView } from "./NotesView";

interface NotesFieldProps {
  id: string;
  value: string;
  onChange: (value: string) => void;
  "aria-label"?: string;
  "aria-invalid"?: boolean;
  "aria-describedby"?: string;
}

/** A multi-line notes field that takes Markdown, with a preview of the result. */
export function NotesField({ value, onChange, ...fieldProps }: NotesFieldProps) {
  const [previewing, setPreviewing] = useState(false);
  const showPreview = previewing && value.trim() !== "";
  return (
    <div className="flex flex-col gap-1">
      {showPreview ? (
        <NotesView text={value} className="min-h-16 rounded-md border border-input px-3 py-2" />
      ) : (
        <Textarea
          {...fieldProps}
          rows={3}
          value={value}
          onChange={(e) => onChange(e.target.value)}
        />
      )}
      <div className="flex items-center justify-between gap-2 text-xs text-muted-foreground">
        <span>Markdown supported</span>
        <Button
          type="button"
          variant="ghost"
          size="sm"
          disabled={!showPreview && value.trim() === ""}
          onClick={() => setPreviewing(!showPreview)}
        >
          {showPreview ? "Edit" : "Preview"}
        </Button>
      </div>
    </div>
  );
}
