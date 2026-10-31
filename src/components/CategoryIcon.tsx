import {
  Archive,
  File,
  FileText,
  Film,
  Image as ImageIcon,
  Music,
} from "lucide-react";
import type { Category } from "../lib/api";

const ICONS: Record<Category, typeof File> = {
  image: ImageIcon,
  audio: Music,
  video: Film,
  document: FileText,
  archive: Archive,
  other: File,
};

export function CategoryIcon({
  category,
  size = 16,
}: {
  category: Category;
  size?: number;
}) {
  const Icon = ICONS[category] ?? File;
  return <Icon size={size} aria-hidden="true" />;
}
