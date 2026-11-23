/// <reference types="@raycast/api">

/* 🚧 🚧 🚧
 * This file is auto-generated from the extension's manifest.
 * Do not modify manually. Instead, update the `package.json` file.
 * 🚧 🚧 🚧 */

/* eslint-disable @typescript-eslint/ban-types */

type ExtensionPreferences = {
  /** Notify When Tab Is Focused - When disabled (default), notifications only fire for background tabs, side-by-side windows, or when another app is active. */
  "notifyWhenFocused": boolean,
  /** Auto-Download ZIP Artifacts - Synced live with the local ContextBraid daemon and browser extension. */
  "autoDownloadZip": boolean,
  /** Favorite Code Editor - Default editor used when opening repositories from the cockpit. */
  "favoriteEditor": "zed" | "cursor" | "vscode" | "windsurf" | "neovim",
  /** Default Alert Mute Duration - Duration used when muting repository notifications (Cmd+M). */
  "muteDurationMinutes": "15" | "30" | "60" | "240"
}

/** Preferences accessible in all the extension's commands */
declare type Preferences = ExtensionPreferences

declare namespace Preferences {
  /** Preferences accessible in the `index` command */
  export type Index = ExtensionPreferences & {}
  /** Preferences accessible in the `cycle-project` command */
  export type CycleProject = ExtensionPreferences & {}
  /** Preferences accessible in the `jump-alert` command */
  export type JumpAlert = ExtensionPreferences & {}
  /** Preferences accessible in the `menu-bar` command */
  export type MenuBar = ExtensionPreferences & {}
}

declare namespace Arguments {
  /** Arguments passed to the `index` command */
  export type Index = {}
  /** Arguments passed to the `cycle-project` command */
  export type CycleProject = {}
  /** Arguments passed to the `jump-alert` command */
  export type JumpAlert = {}
  /** Arguments passed to the `menu-bar` command */
  export type MenuBar = {}
}

