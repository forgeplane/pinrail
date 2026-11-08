// A paperclip that picks files from disk, beside a field that takes them.

import { Paperclip } from "lucide-react";
import { useRef } from "react";
import { Tooltip } from "./Tooltip";

export function AttachButton({
  label,
  onFiles,
  disabled = false,
}: {
  /** what the files go to, such as "Attach a file to the description" */
  label: string;
  onFiles: (files: File[]) => void;
  disabled?: boolean;
}) {
  const picker = useRef<HTMLInputElement>(null);
  return (
    <>
      <Tooltip label={label} side="top">
        <button
          type="button"
          className="bar-button attach-button"
          aria-label={label}
          disabled={disabled}
          onClick={() => picker.current?.click()}
        >
          <Paperclip size={14} />
        </button>
      </Tooltip>
      <input
        ref={picker}
        type="file"
        multiple
        hidden
        aria-label={label}
        onChange={(e) => {
          onFiles(Array.from(e.target.files ?? []));
          e.target.value = "";
        }}
      />
    </>
  );
}
