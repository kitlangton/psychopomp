use std::{env, path::PathBuf};

fn main() -> anyhow::Result<()> {
    let output = env::args().nth(1).map_or_else(
        || PathBuf::from("target/type-and-motion.json"),
        PathBuf::from,
    );
    psychopomp_type_and_motion::build_plan()?.write_or_print(Some(&output))?;
    eprintln!("Wrote {}", output.display());
    Ok(())
}
