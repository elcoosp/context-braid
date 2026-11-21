import { Icon, Image } from "@raycast/api";

export const BRAND_ICONS: Record<string, Image.ImageLike> = {
  zed: "brand-zed.png",
  cursor: "brand-cursor.png",
  vscode: "brand-vscode.png",
  neovim: "brand-neovim.png",
  zai: "brand-zai.png",
  qwen: "brand-qwen.png",
  gemini: "brand-gemini.png",
  deepseek: "brand-deepseek.png",
  browser: Icon.Globe,
  alert: "badge-alert.png",
  completed: "badge-completed.png",
};

export function editorMeta(favorite: string): {
  label: string;
  icon: Image.ImageLike;
} {
  const f = (favorite || "zed").toLowerCase();
  if (f === "cursor") return { label: "Cursor", icon: "brand-cursor.png" };
  if (f === "vscode" || f === "code")
    return { label: "VS Code", icon: "brand-vscode.png" };
  if (f === "windsurf") return { label: "Windsurf", icon: Icon.Code };
  if (f === "neovim" || f === "nvim")
    return { label: "Neovim", icon: "brand-neovim.png" };
  return { label: "Zed", icon: "brand-zed.png" };
}

export function browserBadge(
  browserName: string,
  favoriteEditor = "zed",
): { icon: Image.ImageLike; label: string } {
  const b = browserName.toLowerCase();
  if (b === "brave") return { label: "Brave", icon: "brand-brave.png" };
  if (b === "chrome") return { label: "Chrome", icon: "brand-chrome.png" };
  if (b === "firefox") return { label: "Firefox", icon: "brand-firefox.png" };
  if (b === "editor" || b === "zed") return editorMeta(favoriteEditor);
  return { label: browserName, icon: Icon.Globe };
}
