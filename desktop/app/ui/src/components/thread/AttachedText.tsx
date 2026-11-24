// A message or a prompt with the files attached to it shown as a note's
// files are, not as their paths: a chip for each, an image's thumbnail
// opening it in a pop-up, and a chip for each image the agent shows as
// [Image #N].

import { Image } from "lucide-react";
import { useEffect, useState } from "react";
import { inTauri, serverUrl } from "../../api/client";
import { attachedParts, isImage, type AttachedFile } from "../../lib/attachedPaths";
import { revealFile } from "../../lib/native";
import { NoteFiles } from "../NoteFiles";

/** Where the thread reads an attached file from. */
function useFileUrl(): (file: AttachedFile) => string | null {
  const [base, setBase] = useState<string | null>(null);
  useEffect(() => {
    void serverUrl().then(setBase);
  }, []);
  return (file) => {
    if (!base) return null;
    const seg = encodeURIComponent;
    return `${base}/api/v1/sessions/${seg(file.agent)}/${seg(file.session)}/files/${seg(file.name)}`;
  };
}

/** A file's type, from its name: enough to tell an image from the rest. */
const typeOf = (name: string) =>
  isImage(name) ? `image/${name.split(".").pop()!.toLowerCase()}` : "application/octet-stream";

/** A text's attached files as a note's files are shown, with a button that
 *  shows each in Finder (in a browser, downloads it), and a chip for each
 *  image the agent shows as [Image #N]. Nothing when it has none. */
export function AttachedBadges({ files, images }: { files: AttachedFile[]; images: number }) {
  const url = useFileUrl();
  if (!files.length && !images) return null;
  const byName = new Map(files.map((file) => [file.name, file]));
  const urlOf = (name: string) => {
    const file = byName.get(name);
    return file ? url(file) : null;
  };
  const native = inTauri();
  const open = (name: string) => {
    const file = byName.get(name);
    if (!file) return;
    if (native) {
      revealFile(file.path);
      return;
    }
    const href = url(file);
    if (!href) return;
    const link = document.createElement("a");
    link.href = href;
    link.download = name;
    link.rel = "noreferrer";
    document.body.append(link);
    link.click();
    link.remove();
  };
  return (
    <span className="attached-badges">
      <NoteFiles
        files={files.map((file) => ({ name: file.name, media_type: typeOf(file.name) }))}
        url={urlOf}
        onSave={open}
        action={native ? "reveal" : "save"}
      />
      {images ? (
        <ul className="note-files" aria-label="Images the agent attached">
          {Array.from({ length: images }, (_, i) => (
            <li key={i} className="note-file is-placeholder" title="An image the agent attached">
              <Image size={14} className="note-file-icon" aria-hidden />
              <span className="note-file-name">Image</span>
            </li>
          ))}
        </ul>
      ) : null}
    </span>
  );
}

/** A text with its attached paths and image placeholders as badges. */
export function AttachedText({ text }: { text: string }) {
  const parts = attachedParts(text);
  return (
    <>
      {parts.text}
      <AttachedBadges files={parts.files} images={parts.images} />
    </>
  );
}
