import { useEffect, useRef, useState } from "react";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { FolderOpen, UploadCloud } from "lucide-react";
import { isTauri } from "../lib/api";
import { isAndroid, pickFiles } from "../lib/platform";

export interface DropzoneProps {
  onFiles: (paths: string[]) => void;
  active: boolean;
  multiple?: boolean;
  filters?: { name: string; extensions: string[] }[];
  /** Android picker scope. */
  kind?: "video" | "media" | "any";
  title: string;
  subtitle?: string;
  icon?: "upload" | "folder";
  compact?: boolean;
  disabled?: boolean;
}

export function Dropzone({
  onFiles,
  active,
  multiple = true,
  filters,
  kind = "media",
  title,
  subtitle,
  icon = "upload",
  compact = false,
  disabled = false,
}: DropzoneProps) {
  const [dragging, setDragging] = useState(false);
  const onFilesRef = useRef(onFiles);
  onFilesRef.current = onFiles;

  useEffect(() => {
    if (!active || !isTauri || isAndroid) {
      setDragging(false);
      return;
    }
    let mounted = true;
    let unlisten: (() => void) | undefined;
    getCurrentWebview()
      .onDragDropEvent((event) => {
        const payload = event.payload;
        if (payload.type === "enter" || payload.type === "over") {
          setDragging(true);
        } else if (payload.type === "leave") {
          setDragging(false);
        } else if (payload.type === "drop") {
          setDragging(false);
          if (!disabled && payload.paths.length > 0) {
            onFilesRef.current(multiple ? payload.paths : payload.paths.slice(0, 1));
          }
        }
      })
      .then((fn) => {
        if (mounted) unlisten = fn;
        else fn();
      })
      .catch(() => {
        /* Drag and drop only exists inside the Tauri webview. */
      });
    return () => {
      mounted = false;
      unlisten?.();
    };
  }, [active, multiple, disabled]);

  const browse = async () => {
    if (disabled) return;
    try {
      const paths = await pickFiles({ multiple, kind, filters, title });
      if (paths.length > 0) {
        onFilesRef.current(multiple ? paths : paths.slice(0, 1));
      }
    } catch {
      /* Dialog cancelled or unavailable. */
    }
  };

  return (
    <button
      type="button"
      className={`dropzone ${dragging ? "is-dragging" : ""} ${compact ? "dropzone--compact" : ""}`}
      onClick={browse}
      disabled={disabled}
      aria-label={title}
    >
      <span className="dropzone__icon">
        {icon === "folder" ? <FolderOpen size={24} /> : <UploadCloud size={24} />}
      </span>
      <span className="dropzone__title">{title}</span>
      {subtitle && <span className="dropzone__sub">{subtitle}</span>}
      <span className="dropzone__cta">{isAndroid ? "タップしてファイルを選択" : "クリックして選択"}</span>
    </button>
  );
}
