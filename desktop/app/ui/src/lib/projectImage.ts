// A project's avatar image: the PNG a person drops or picks, cropped to its
// middle square and shrunk to the size the app sends, and where the core
// serves it.

import { useEffect, useState } from "react";
import { serverUrl } from "../api/client";

/** The side, in pixels, of the PNG the app sends. */
export const IMAGE_SIDE = 256;

/** The square in the middle of an image, as the part to draw: its corner and side. */
export function middleSquare(width: number, height: number) {
  const side = Math.min(width, height);
  return { x: (width - side) / 2, y: (height - side) / 2, side };
}

/** A PNG file cropped to its middle square and drawn at IMAGE_SIDE pixels. */
export async function squarePng(file: Blob): Promise<Blob> {
  if (file.type !== "image/png") throw new Error("Choose a PNG image.");
  const bitmap = await createImageBitmap(file);
  const { x, y, side } = middleSquare(bitmap.width, bitmap.height);
  const canvas = document.createElement("canvas");
  canvas.width = canvas.height = IMAGE_SIDE;
  const context = canvas.getContext("2d")!;
  context.imageSmoothingQuality = "high";
  context.drawImage(bitmap, x, y, side, side, 0, 0, IMAGE_SIDE, IMAGE_SIDE);
  bitmap.close();
  return new Promise((resolve, reject) =>
    canvas.toBlob((blob) => (blob ? resolve(blob) : reject(new Error("The image could not be read."))), "image/png"),
  );
}

/** The core's address, once it is known. */
export function useServerUrl(): string | null {
  const [base, setBase] = useState<string | null>(null);
  useEffect(() => {
    let current = true;
    void serverUrl().then((url) => current && setBase(url));
    return () => {
      current = false;
    };
  }, []);
  return base;
}

/** Where the core serves a project's image; the hash in it makes a new image a new address. */
export const imageUrl = (base: string, project: { id: string; image?: string | null }) =>
  project.image ? `${base}/api/v1/projects/${encodeURIComponent(project.id)}/image?v=${project.image}` : null;
