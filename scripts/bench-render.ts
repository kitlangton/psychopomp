// Render and snapshot throughput benchmark on bounded representative windows of
// `output/weekly-ten/plan.json` (1920x1080, 60 fps, 24 temporal samples, neutral theme).
//
// Usage:
//   bun scripts/bench-render.ts <label> [--runs 7]
import { mkdir, rm, writeFile } from "node:fs/promises"
import path from "node:path"

const args = Bun.argv.slice(2)
const label = args.find((arg, i) => !arg.startsWith("--") && args[i - 1] !== "--runs") ?? "run"
const runsIndex = args.indexOf("--runs")
const runs = runsIndex >= 0 ? Number(args[runsIndex + 1]) : 7
if (!Number.isInteger(runs) || runs < 1) throw new Error("--runs must be a positive integer")

const root = path.join(import.meta.dir, "..")
const binary = path.join(root, "target/release/psychopomp")
const plan = path.join(root, "output/weekly-ten/plan.json")
const outDir = path.join(root, "output/render-research", label)
await mkdir(outDir, { recursive: true })

function median(values: number[]): number {
  const sorted = [...values].sort((a, b) => a - b)
  const mid = Math.floor(sorted.length / 2)
  return sorted.length % 2 === 1 ? sorted[mid] : (sorted[mid - 1] + sorted[mid]) * 0.5
}

function mad(values: number[]): number {
  const m = median(values)
  return median(values.map((v) => Math.abs(v - m)))
}

function stats(values: number[]) {
  return {
    median: Number(median(values).toFixed(2)),
    mad: Number(mad(values).toFixed(2)),
    min: Number(Math.min(...values).toFixed(2)),
    max: Number(Math.max(...values).toFixed(2)),
    samples: values.map((v) => Number(v.toFixed(2))),
  }
}

function runRender(range: string, name: string): number {
  const mp4 = path.join(outDir, `${name}.mp4`)
  const t0 = performance.now()
  const res = Bun.spawnSync(
    [binary, "plan", "render", plan, mp4, "--range", range, "--theme", "neutral"],
    { cwd: root, env: { ...process.env, PSYCHOPOMP_RENDER_SLOTS: "0" }, stdout: "pipe", stderr: "pipe" },
  )
  const dt = performance.now() - t0
  if (res.exitCode !== 0) {
    throw new Error(`render ${range} failed:\n${res.stderr.toString()}`)
  }
  return dt
}

function runSnapshot(): number {
  const snapDir = path.join(outDir, "snap")
  const t0 = performance.now()
  const res = Bun.spawnSync(
    [binary, "plan", "snapshot", plan, "1.0,3.5,4.3,5.2,6.8,9.4", snapDir, "--shutter", "--theme", "neutral"],
    { cwd: root, stdout: "pipe", stderr: "pipe" },
  )
  const dt = performance.now() - t0
  if (res.exitCode !== 0) {
    throw new Error(`snapshot failed:\n${res.stderr.toString()}`)
  }
  return dt
}

const quietSamples: number[] = []
const burstSamples: number[] = []
const confettiSamples: number[] = []
const combinedSamples: number[] = []
const snapshotSamples: number[] = []

for (let i = 0; i <= runs; i++) {
  const quiet = runRender("3.15..3.65", "quiet")
  const burst = runRender("4.15..4.65", "burst")
  const confetti = runRender("5.10..5.60", "confetti")
  const snap = runSnapshot()
  if (i > 0) {
    quietSamples.push(quiet)
    burstSamples.push(burst)
    confettiSamples.push(confetti)
    combinedSamples.push(quiet + burst + confetti)
    snapshotSamples.push(snap)
  }
}

await rm(path.join(outDir, "snap"), { recursive: true, force: true })

const result = {
  label,
  runs,
  render_windows_ms: stats(combinedSamples),
  burst_window_ms: stats(burstSamples),
  confetti_window_ms: stats(confettiSamples),
  quiet_window_ms: stats(quietSamples),
  snapshot_shutter_ms: stats(snapshotSamples),
}

await writeFile(path.join(outDir, "result.json"), JSON.stringify(result, null, 2) + "\n")
console.log(JSON.stringify(result, null, 2))
console.log(`METRIC render_windows_ms=${result.render_windows_ms.median.toFixed(2)}`)
console.log(`METRIC burst_window_ms=${result.burst_window_ms.median.toFixed(2)}`)
console.log(`METRIC confetti_window_ms=${result.confetti_window_ms.median.toFixed(2)}`)
console.log(`METRIC quiet_window_ms=${result.quiet_window_ms.median.toFixed(2)}`)
console.log(`METRIC snapshot_shutter_ms=${result.snapshot_shutter_ms.median.toFixed(2)}`)
