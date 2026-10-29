// Labeled contact sheet of exact frames from a Scene Plan or reel: the fastest way
// to review choreography before a full render. Frames render in one process
// (`psychopomp plan snapshot`); `--shutter` exposes each like an exported video
// frame, motion blur included. `--crop x,y,w,h` tiles one region at full size.
//
// Usage:
//   bun scripts/sheet.ts <plan-or-reel.json> <from:to:step | t1,t2,...> [--theme opencode]
//     [--cols 4] [--crop x,y,w,h] [--shutter] [--out output/sheet.jpg]
import { rm } from "node:fs/promises"
import path from "node:path"

const args = Bun.argv.slice(2)
const valued = ["--theme", "--cols", "--out", "--crop"]
const flag = (name: string, fallback: string) => (args.includes(name) ? args[args.indexOf(name) + 1] : fallback)
const positional = args.filter((arg, index) => !arg.startsWith("--") && !valued.includes(args[index - 1]))
const [plan, spec] = positional
if (!plan || !spec)
  throw new Error(
    "usage: bun scripts/sheet.ts <plan-or-reel.json> <from:to:step | t1,t2,...> [--theme NAME] [--cols N] [--crop x,y,w,h] [--shutter] [--out FILE]",
  )
const theme = flag("--theme", "original")
const cols = Number(flag("--cols", "4"))
const out = flag("--out", "output/sheet.jpg")
const crop = args.includes("--crop") ? flag("--crop", "").split(",").map(Number) : undefined
if (crop && (crop.length !== 4 || crop.some(Number.isNaN))) throw new Error("--crop takes x,y,w,h")

const root = path.join(import.meta.dir, "..")
const targetDir = process.env.CARGO_TARGET_DIR ?? path.join(root, "target")
const binary = path.join(targetDir, "release/psychopomp")
if (Bun.spawnSync(["cargo", "build", "--release", "-q", "-p", "psychopomp-render"], { cwd: root, stderr: "inherit" }).exitCode !== 0) throw new Error("build failed")
const frames = path.join(root, "output/sheet-frames")
await rm(frames, { recursive: true, force: true })
const snapshot = Bun.spawnSync(
  [binary, "plan", "snapshot", plan, spec, frames, "--theme", theme, ...(args.includes("--shutter") ? ["--shutter"] : [])],
  { stdout: "pipe", stderr: "pipe" },
)
if (snapshot.exitCode !== 0) throw new Error(snapshot.stderr.toString())
const rendered: { time: number; path: string }[] = JSON.parse(snapshot.stdout.toString()).frames

// Label each tile with its time; whole frames tile at half resolution.
const geometry = crop ? `${crop[2]}x${crop[3]}+6+6` : "960x540+6+6"
const montage = Bun.spawnSync(
  [
    "magick", "montage",
    ...rendered.flatMap(({ time, path: file }) => [
      "-label", `${time.toFixed(2)}s`,
      crop ? `${file}[${crop[2]}x${crop[3]}+${crop[0]}+${crop[1]}]` : file,
    ]),
    "-tile", `${cols}x`, "-geometry", geometry, "-background", "#111", "-fill", "#ddd",
    "-font", "/System/Library/Fonts/Menlo.ttc", "-pointsize", "22", path.resolve(out),
  ],
  { stderr: "pipe" },
)
if (montage.exitCode !== 0) throw new Error(montage.stderr.toString())
console.log(JSON.stringify({ sheet: path.resolve(out), frames: rendered.length }))
