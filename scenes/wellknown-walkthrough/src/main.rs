use std::{env, fs, path::PathBuf};

fn main() -> anyhow::Result<()> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let narration = root.join("narration");
    let only = env::args().nth(1);
    let reels = [
        (
            "pr-53821.reel.json",
            psychopomp_wellknown_walkthrough::build_53821
                as fn(&std::path::Path) -> anyhow::Result<psychopomp::plan::ReelPlan>,
        ),
        (
            "pr-53822.reel.json",
            psychopomp_wellknown_walkthrough::build_53822,
        ),
        (
            "pr-53823.reel.json",
            psychopomp_wellknown_walkthrough::build_53823,
        ),
    ];
    for (file, build) in reels {
        if only
            .as_deref()
            .is_some_and(|prefix| !file.starts_with(prefix))
        {
            continue;
        }
        let reel = build(&narration)?;
        let output = root.join(file);
        fs::write(&output, serde_json::to_string_pretty(&reel)? + "\n")?;
        eprintln!(
            "wrote {} ({:.1}s, {} segments)",
            output.display(),
            reel.duration_nanos() as f64 / 1e9,
            reel.segments.len()
        );
    }
    Ok(())
}
