/**
 * A file's icon and colour, by what it is.
 *
 * The point is **scanning**, not decoration. A tree of forty identical grey
 * document glyphs makes you read every filename; a tree where Rust is orange
 * and config is grey and tests are green lets you find the file by shape and
 * only then read it. That is the whole reason IntelliJ's Material theme is
 * worth copying here.
 *
 * Two rules keep it from becoming a rainbow:
 *
 * - **Colour by language family, not by extension.** `.ts` and `.tsx` are the
 *   same colour because they are the same thing to you; `.json` and `.toml`
 *   are both config.
 * - **Every colour comes from the palette.** `R-J6`'s two hand-tested palettes
 *   are the only source, so the tree stays legible in light mode without a
 *   second set of decisions — and the graph ramp is already chosen to be
 *   distinguishable from itself.
 */

import {
  Binary,
  BookText,
  Braces,
  Container,
  Database,
  FileArchive,
  FileCode,
  FileCog,
  FileImage,
  FileJson,
  FileLock,
  FileTerminal,
  FileText,
  FileType,
  Folder,
  FolderOpen,
  GitBranch,
  Hash,
  Package,
  Palette,
  Settings,
  type LucideIcon,
} from "lucide-react";

interface Look {
  Icon: LucideIcon;
  color: string;
}

const CODE: Look = { Icon: FileCode, color: "var(--graph-0)" };
const CONFIG: Look = { Icon: Settings, color: "var(--dim)" };
const DOC: Look = { Icon: BookText, color: "var(--graph-1)" };
const DATA: Look = { Icon: FileJson, color: "var(--graph-2)" };
const STYLE: Look = { Icon: Palette, color: "var(--graph-5)" };
const SHELL: Look = { Icon: FileTerminal, color: "var(--graph-4)" };
const ASSET: Look = { Icon: FileImage, color: "var(--graph-3)" };
const PLAIN: Look = { Icon: FileText, color: "var(--dim)" };

/** Whole filenames win over extensions — `Cargo.toml` is not just "some toml". */
const BY_NAME: Record<string, Look> = {
  "cargo.toml": { Icon: Package, color: "var(--urgent)" },
  "cargo.lock": { Icon: FileLock, color: "var(--dim)" },
  "package.json": { Icon: Package, color: "var(--graph-1)" },
  "package-lock.json": { Icon: FileLock, color: "var(--dim)" },
  "pnpm-lock.yaml": { Icon: FileLock, color: "var(--dim)" },
  "tsconfig.json": { Icon: FileCog, color: "var(--graph-0)" },
  dockerfile: { Icon: Container, color: "var(--graph-0)" },
  makefile: { Icon: FileCog, color: "var(--graph-2)" },
  ".gitignore": { Icon: GitBranch, color: "var(--urgent)" },
  ".gitattributes": { Icon: GitBranch, color: "var(--urgent)" },
  "readme.md": { Icon: BookText, color: "var(--graph-0)" },
  "license": { Icon: FileText, color: "var(--graph-2)" },
  "claude.md": { Icon: BookText, color: "var(--purple)" },
  "agents.md": { Icon: BookText, color: "var(--purple)" },
  "status.md": { Icon: BookText, color: "var(--dim)" },
};

const BY_EXT: Record<string, Look> = {
  // Systems and application languages share the code glyph; the colour is what
  // tells them apart at a glance.
  rs: { Icon: FileCode, color: "var(--urgent)" },
  go: { Icon: FileCode, color: "var(--graph-4)" },
  c: CODE,
  h: CODE,
  cc: CODE,
  cpp: CODE,
  hpp: CODE,
  java: { Icon: FileCode, color: "var(--red)" },
  kt: { Icon: FileCode, color: "var(--purple)" },
  swift: { Icon: FileCode, color: "var(--urgent)" },
  py: { Icon: FileCode, color: "var(--graph-2)" },
  rb: { Icon: FileCode, color: "var(--red)" },
  php: { Icon: FileCode, color: "var(--purple)" },
  cs: { Icon: FileCode, color: "var(--graph-1)" },

  ts: { Icon: FileCode, color: "var(--graph-0)" },
  tsx: { Icon: FileCode, color: "var(--graph-0)" },
  js: { Icon: FileCode, color: "var(--graph-2)" },
  jsx: { Icon: FileCode, color: "var(--graph-2)" },
  mjs: { Icon: FileCode, color: "var(--graph-2)" },
  cjs: { Icon: FileCode, color: "var(--graph-2)" },

  json: DATA,
  jsonc: DATA,
  yaml: { Icon: Braces, color: "var(--graph-5)" },
  yml: { Icon: Braces, color: "var(--graph-5)" },
  toml: { Icon: Settings, color: "var(--graph-2)" },
  ini: CONFIG,
  conf: CONFIG,
  env: { Icon: FileLock, color: "var(--amber)" },

  md: DOC,
  mdx: DOC,
  txt: PLAIN,
  rst: DOC,
  adoc: DOC,

  css: STYLE,
  scss: STYLE,
  sass: STYLE,
  less: STYLE,
  html: { Icon: FileCode, color: "var(--urgent)" },
  svg: { Icon: FileImage, color: "var(--graph-3)" },

  sh: SHELL,
  bash: SHELL,
  zsh: SHELL,
  fish: SHELL,
  ps1: SHELL,

  sql: { Icon: Database, color: "var(--graph-4)" },
  db: { Icon: Database, color: "var(--dim)" },
  sqlite: { Icon: Database, color: "var(--dim)" },

  png: ASSET,
  jpg: ASSET,
  jpeg: ASSET,
  gif: ASSET,
  webp: ASSET,
  ico: ASSET,

  zip: { Icon: FileArchive, color: "var(--dim)" },
  gz: { Icon: FileArchive, color: "var(--dim)" },
  tar: { Icon: FileArchive, color: "var(--dim)" },
  lock: { Icon: FileLock, color: "var(--dim)" },
  bin: { Icon: Binary, color: "var(--dim)" },
  wasm: { Icon: Binary, color: "var(--purple)" },
};

/** Directories worth recognising — the ones you skim past rather than into. */
const BY_DIR: Record<string, string> = {
  ".git": "var(--urgent)",
  node_modules: "var(--noise)",
  target: "var(--noise)",
  dist: "var(--noise)",
  build: "var(--noise)",
  src: "var(--graph-0)",
  crates: "var(--urgent)",
  tests: "var(--graph-1)",
  test: "var(--graph-1)",
  docs: "var(--graph-0)",
  scripts: "var(--graph-4)",
  assets: "var(--graph-3)",
  public: "var(--graph-3)",
};

export function lookFor(name: string): Look {
  const lower = name.toLowerCase();
  const byName = BY_NAME[lower];
  if (byName) return byName;
  // `.d.ts` and friends: take the *last* extension, but let a compound one win.
  if (lower.endsWith(".d.ts")) return { Icon: FileType, color: "var(--graph-0)" };
  if (/\.(test|spec)\.[a-z]+$/.test(lower)) return { Icon: Hash, color: "var(--graph-1)" };
  const ext = lower.slice(lower.lastIndexOf(".") + 1);
  return BY_EXT[ext] ?? PLAIN;
}

/**
 * The icon for one entry of a tree or a tab strip.
 *
 * `dimmed` is for gitignored subtrees — generated noise should look like noise,
 * which is a rule the Rust client already followed and the colour must not
 * quietly undo.
 */
export function FileIcon({
  name,
  isDir,
  open,
  size = 12,
  dimmed,
  className,
}: {
  name: string;
  isDir?: boolean;
  open?: boolean;
  size?: number;
  dimmed?: boolean;
  className?: string;
}) {
  if (isDir) {
    const Icon = open ? FolderOpen : Folder;
    const color = BY_DIR[name.toLowerCase()] ?? "var(--dim)";
    return (
      <Icon
        size={size}
        className={className}
        style={{ color: dimmed ? "var(--noise)" : color, opacity: dimmed ? 0.6 : 1 }}
      />
    );
  }
  const { Icon, color } = lookFor(name);
  return (
    <Icon
      size={size}
      className={className}
      style={{ color: dimmed ? "var(--noise)" : color, opacity: dimmed ? 0.6 : 1 }}
    />
  );
}
