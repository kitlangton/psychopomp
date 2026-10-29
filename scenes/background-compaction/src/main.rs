use std::{env, path::PathBuf};

fn main() -> anyhow::Result<()> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let output = env::args()
        .nth(1)
        .map_or_else(|| root.join("background-compaction.json"), PathBuf::from);
    psychopomp_background_compaction::build_plan(&root.join("narration"))?.write_or_print(Some(&output))?;
    eprintln!("Wrote {}", output.display());
    Ok(())
}
