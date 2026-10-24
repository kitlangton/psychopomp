// Narration for a Scene Program: synthesize each clip in Kit's voice on ElevenLabs v4,
// loudness-normalize it, transcribe word timings with Whisper, and write a manifest
// with exact durations. Only clips whose text changed are regenerated.
//
// Usage:
//   ELEVENLABS_API_KEY=... bun scripts/narrate.ts scenes/<scene>/narration/script.json
//   bun scripts/narrate.ts <script.json> --only intro,outro   # force selected clips
//   bun scripts/narrate.ts <script.json> --draft               # macOS `say`, no credentials
//
// script.json: { "engine"?: "elevenlabs" | "fish", "voice"?: string, "speed"?: number,
//   "model"?: string, "settings"?: { "stability": number, "similarity": number },
//   "loudness"?: "dynamic" | "linear", "clips": [{ "id": string, "text": string }] }
// The default is `eleven_v4` through Text to Dialogue with Kit's Professional Voice
// Clone at stability 0.5 and similarity 0.75. "fish" is retired; it remains only so
// old Fish narration can still be adopted.
// Writes next to the script: <id>.mp3, <id>.words.json ({ wordTimings: [...] }), narration.json.
// Bracketed delivery cues such as "[confident]" are spoken as direction, not words.
// "dynamic" loudness (the default) evens the level within each clip; "linear" applies
// one gain per clip and limits peaks, so a whisper that builds to a scream keeps its swell.
//
// Scene Programs can instead declare narration with the psychopomp-media crate
// (SCENE_PLANS.md, "Declare Narration And Sound"), which keeps state in
// media.lock.json; `cargo run -p psychopomp-media -- adopt <narration-dir>`
// moves a scene voiced by this script into that lock without regenerating it.
import { createHash } from "node:crypto"
import { mkdtemp, rm } from "node:fs/promises"
import { tmpdir } from "node:os"
import path from "node:path"

type Script = {
  engine?: "fish" | "elevenlabs"
  voice?: string
  speed?: number
  model?: string
  settings?: { stability: number; similarity: number }
  loudness?: "dynamic" | "linear"
  clips: { id: string; text: string }[]
}
type Clip = { id: string; file: string; words: string; durationNanos: number; textHash: string; engine: string; model?: string; requestId?: string }

const args = Bun.argv.slice(2)
const scriptPath = args.find((arg) => !arg.startsWith("--"))
if (!scriptPath) throw new Error("usage: bun scripts/narrate.ts <script.json> [--only id,id]")
const only = new Set(args.includes("--only") ? args[args.indexOf("--only") + 1].split(",") : [])
// Draft narration lets a scene be timed before the final voice exists. Phrase-keyed
// cues re-derive themselves when final clips replace the drafts.
const fishSay = process.env.FISH_SAY ?? path.join(process.env.HOME!, ".opencode/skill/fish-audio/scripts/fish-say.ts")
const whisperModel = process.env.WHISPER_MODEL ?? "mlx-community/whisper-large-v3-mlx"

const dir = path.dirname(path.resolve(scriptPath))
const script: Script = await Bun.file(scriptPath).json()
const engine = args.includes("--draft") ? "say" : script.engine ?? "elevenlabs"
if (!["say", "fish", "elevenlabs"].includes(engine)) throw new Error(`unknown narration engine '${engine}'`)
if (engine === "elevenlabs") {
  // Kit's Professional Voice Clone; psychopomp-media's adopt.rs resolves the same defaults.
  script.voice ??= "8olojUk4IXpvKgaOCHXj"
  script.settings ??= { stability: 0.5, similarity: 0.75 }
}
if (engine === "elevenlabs" && !process.env.ELEVENLABS_API_KEY) throw new Error("ElevenLabs requires ELEVENLABS_API_KEY")
if (engine === "elevenlabs" && script.speed !== undefined)
  throw new Error("Eleven v4 uses text directions for pace; speed is not supported")
const model = engine === "elevenlabs" ? script.model ?? "eleven_v4" : undefined
const loudness = script.loudness ?? "dynamic"
if (!["dynamic", "linear"].includes(loudness)) throw new Error(`unknown loudness '${loudness}'`)
const manifestPath = path.join(dir, "narration.json")
const previous: Record<string, Clip> = Object.fromEntries(
  ((await Bun.file(manifestPath).exists()) ? (await Bun.file(manifestPath).json()).clips : []).map((clip: Clip) => [
    clip.id,
    clip,
  ]),
)

const ids = new Set<string>()
const clips: Clip[] = []
for (const clip of script.clips) {
  if (!/^[a-z0-9-]+$/.test(clip.id) || ids.has(clip.id)) throw new Error(`clip id '${clip.id}' must be unique kebab-case`)
  ids.add(clip.id)
  const textHash = createHash("sha256")
    .update(`${engine}|${script.voice ?? ""}|${script.speed ?? 1}|${clip.text}`)
  if (engine === "elevenlabs") textHash.update(JSON.stringify({ model, settings: script.settings }))
  if (loudness !== "dynamic") textHash.update(loudness)
  const hash = textHash
    .digest("hex")
    .slice(0, 16)
  const file = `${clip.id}.mp3`
  const words = `${clip.id}.words.json`
  const cached = previous[clip.id]
  const fresh =
    cached?.textHash === hash &&
    !only.has(clip.id) &&
    (await Bun.file(path.join(dir, file)).exists()) &&
    (await Bun.file(path.join(dir, words)).exists())
  if (fresh) {
    clips.push(cached)
    continue
  }
  console.error(`narrating ${clip.id}`)
  const work = await mkdtemp(path.join(tmpdir(), "narrate-"))
  try {
    const text = path.join(work, "text.txt")
    const raw = path.join(work, "raw.wav")
    let requestId: string | undefined
    await Bun.write(text, engine === "say" ? clip.text.replace(/\[[^\]]*\]\s*/g, "") : clip.text)
    if (engine === "say")
      run(["say", "-v", process.env.SAY_VOICE ?? "Samantha", "-f", text, "-o", raw, "--data-format=LEI16@48000"])
    else if (engine === "elevenlabs") {
      // One speaker through Text to Dialogue: the same endpoint supports future
      // multi-voice scenes, but this script deliberately owns one narrator.
      if (clip.text.length > 2000) throw new Error(`clip '${clip.id}' exceeds the reliable 2,000-character dialogue limit`)
      const response = await fetch("https://api.elevenlabs.io/v1/text-to-dialogue?output_format=mp3_44100_192", {
        method: "POST",
        headers: { "xi-api-key": process.env.ELEVENLABS_API_KEY!, "Content-Type": "application/json" },
        body: JSON.stringify({ model_id: model, inputs: [{ voice_id: script.voice, text: clip.text }], settings: script.settings, use_pvc_as_ivc: false }),
        signal: AbortSignal.timeout(120_000),
      })
      if (!response.ok) {
        const error = await response.json().catch(() => ({})) as { detail?: { status?: string } }
        throw new Error(`ElevenLabs HTTP ${response.status}: ${error.detail?.status ?? "generation failed"}`)
      }
      if (!response.headers.get("content-type")?.startsWith("audio/")) throw new Error("ElevenLabs did not return audio")
      requestId = response.headers.get("request-id") ?? undefined
      await Bun.write(raw, await response.arrayBuffer())
    } else
      run([
        "bun",
        fishSay,
        "--file",
        text,
        "--out",
        raw,
        ...(script.voice ? ["--voice", script.voice] : []),
        ...(script.speed ? ["--speed", String(script.speed)] : []),
      ])
    // One loudness target for every clip keeps the voice even across segments.
    const normalize =
      loudness === "linear"
        ? `volume=${(-16 - integratedLoudness(raw)).toFixed(2)}dB,alimiter=limit=0.84:level=0:latency=1`
        : "loudnorm=I=-16:TP=-1.5:LRA=11"
    run(["ffmpeg", "-y", "-loglevel", "error", "-i", raw, "-af", normalize, "-ar", "48000", "-ac", "1", "-b:a", "160k", path.join(dir, file)])
    run([
      "uvx", "--from", "mlx-whisper", "mlx_whisper", "--model", whisperModel, "--word-timestamps", "True",
      "--output-format", "json", "--output-dir", work, "--output-name", clip.id, path.join(dir, file),
    ])
    // Whisper writes bare NaN statistics for wordless audio such as a howl.
    const transcript = JSON.parse((await Bun.file(path.join(work, `${clip.id}.json`)).text()).replace(/\bNaN\b/g, "null"))
    let previousEnd = 0
    const wordTimings = transcript.segments
      .flatMap((segment: { words: { word: string; start: number; end: number }[] }) => segment.words)
      .map((word: { word: string; start: number; end: number }) => {
        // Whisper can overlap adjacent words by a few milliseconds; psychopomp requires order.
        const start = Math.max(word.start, previousEnd)
        const end = Math.max(word.end, start)
        previousEnd = end
        return { word: word.word.trim(), start, end }
      })
    await Bun.write(path.join(dir, words), JSON.stringify({ wordTimings }, null, 1) + "\n")
    clips.push({ id: clip.id, file, words, durationNanos: durationNanos(path.join(dir, file)), textHash: hash, engine, model, requestId })
    // Preserve completed paid generations if a later clip fails.
    const checkpoint = script.clips.flatMap(({ id }) => {
      const saved = clips.find((item) => item.id === id) ?? previous[id]
      return saved ? [saved] : []
    })
    await Bun.write(manifestPath, JSON.stringify({ clips: checkpoint }, null, 2) + "\n")
  } finally {
    await rm(work, { recursive: true, force: true })
  }
}

await Bun.write(manifestPath, JSON.stringify({ clips }, null, 2) + "\n")
console.log(JSON.stringify(clips.map((clip) => ({ id: clip.id, engine: clip.engine, seconds: clip.durationNanos / 1e9 }))))

function run(command: string[]) {
  const result = Bun.spawnSync(command, { stderr: "pipe", stdout: "pipe" })
  if (result.exitCode !== 0) throw new Error(`${command[0]} failed: ${result.stderr.toString()}`)
  return result.stdout.toString()
}

function integratedLoudness(file: string) {
  const result = Bun.spawnSync(["ffmpeg", "-hide_banner", "-nostats", "-i", file, "-af", "ebur128", "-f", "null", "-"], { stderr: "pipe" })
  const lufs = result.stderr.toString().match(/Integrated loudness:\s+I:\s+(-?[\d.]+) LUFS/)
  if (result.exitCode !== 0 || !lufs) throw new Error(`cannot measure loudness of ${file}`)
  return Number(lufs[1])
}

// Decoded sample count, not container metadata: MP3 padding makes the latter imprecise.
function durationNanos(file: string) {
  const out = run(["ffprobe", "-v", "error", "-show_entries", "stream=duration_ts,time_base,sample_rate", "-of", "json", file])
  const stream = JSON.parse(out).streams[0]
  const [num, den] = stream.time_base.split("/").map(Number)
  const seconds = (stream.duration_ts * num) / den
  // Stay a millisecond inside the decoded audio so clip ranges never exceed the source.
  return Math.floor((seconds - 0.001) * 1e9)
}
