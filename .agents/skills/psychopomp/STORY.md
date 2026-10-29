# Story, Form, and Anti-Slop Rules (`STORY.md`)

## Narrative arcs that hold attention

- **PR / Bug Explainer (`Before -> Replay -> Receipt`):** Show the broken behavior
  in physical space first, rewind or reset the same actors in place, replay the
  exact moment resolving cleanly, then zoom into the condensed code diff. Viewers
  must see *what* fails before reading *why*.
- **Architecture Comparison (`Context -> Dual Pipeline -> Code Receipt -> Verdict`):**
  Open on the real user prompt or bottleneck (`ChatActor` or `TerminalActor`),
  lay out both pipelines on the Stage so structural cost is visible in motion
  (allocations, hops, missing blur, zapped glue crates), zoom into verbatim source
  receipts with IDE diagnostics (`DiagnosticActor` + `HoverActor`), and close on
  measured numbers or a live `ChecklistActor`.
- **One focal action per beat.** Establish the source, follow the signal, read the
  consequence. Hold background flow still while the viewer reads code or a callout.
  Give a 6–9 word line at least 2.5–3.5 seconds on screen after its entrance settles,
  and leave 1.2–1.8 seconds of breathing room at segment tails before dipping.

## Anti-slop gates

Check every contact sheet (`plan strip`) against these before rendering MP4:

- **Frame 0 is alive.** Show header chrome and the primary surface at `t = 0`;
  avoid starting a video on dead black.
- **No generic box-and-arrow wallpaper.** If swapping the text strings would make
  the diagram fit another product, pick a truer form (actual UI anatomy, a native
  chat thread, a stepped code diff with red error waves, or a physical orb/form
  breakdown).
- **Clear Neutral (`--theme neutral`) palette.** Warm obsidian `#080807` canvas,
  1px graphite borders, warm bone `#e2dfd9` ink, and one semantic accent per beat
  (`Tone::Accent` gold for the hero, `Tone::Error` coral for failure, `Tone::Success`
  sage for resolution, `Tone::Request` blue for neutral signals).
- **Generous margins and optical balance.** Keep content inside `x = 140..1780`,
  `y = 130..950` so headers (`y ~ 72`) and footers (`y ~ 1010`) breathe. Size
  Stage cards wide enough (`330..480 px`) that titles and statuses pass
  `plan check`'s `card_text_overflow` gate without cramping.
- **Continuity.** Elements travel one smooth monotonic path (`plan onion`);
  persistent code tokens keep stable `InlinePart` IDs and slide into place;
  lights cool exponentially rather than cutting off; zero unintended 1-frame pops
  (`plan check --pops`).

## Narration and on-screen copy

- Write spoken lines in plain, natural English: complete connected sentences,
  natural contractions, concrete file names and numbers.
- Strip AI tells: no `"it's not X, it's Y"`, no `"Here's the thing:"` /
  `"Plot twist:"`, no verbless three-fragment punchlines, no `"seamless"` /
  `"robust"` / `"testament"`, and no em dashes.
- Spell acronyms phonetically in spoken strings (`"S V G"`, `"W G P U"`,
  `"sixty F P S"`, `"spring dot R S"`) when TTS would misread them, and keep
  burned-in `SubtitlesActor` off when phonetic spellings are used so on-screen
  captions (`header`, `chip`, `footer`, cards) keep exact code spelling.
- Direct ElevenLabs `eleven_v4` with sparse, specific `[...]` tags immediately
  before clauses (`[calm, deadpan]`, `[dryly amused]`, `[excited, building]`,
  `[playful, direct]`). Always pass fallback slices in `spoken.at_any(&["twenty-four", "twenty four"])`
  so hyphenation differences never break cue lookup.
