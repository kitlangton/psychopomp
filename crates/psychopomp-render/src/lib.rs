//! Deterministic motion-graphics renderer, GPU shutter compositor, and live
//! Studio preview library for Psychopomp.

mod encode;
mod exposure;
mod exposure_gpu;
mod footage;
mod plan_runtime;
mod render;
mod video;

use std::{fs, path::PathBuf};

use anyhow::{Context, Result, bail};

pub use plan_runtime::PreviewSession;
pub use render::Theme;

/// Shared Seer communication primitives (`facts`, `compare`, `steps`, `size`,
/// `md_code`, `> [!note]`, and `rune_kit::Markdown`) used by both `RuneSurface`
/// and `psychopomp-studio`.
pub fn render_seer_note(key_prefix: &str, markdown: &str, kit: &rune_kit::Kit) -> rune::El {
    render::rune_notes::render_note(key_prefix, markdown, kit)
}

pub fn cli_main() -> Result<()> {
    let arguments = std::env::args().skip(1).collect::<Vec<_>>();
    if let [command, rest @ ..] = arguments.as_slice()
        && command == "plan"
    {
        return plan_runtime::command(rest);
    }
    if let [command, rest @ ..] = arguments.as_slice()
        && command == "verify"
    {
        return plan_runtime::verify::command(rest);
    }
    let output = match arguments.as_slice() {
        [] => PathBuf::from("output/psychopomp-prototype.mp4"),
        [output] => PathBuf::from(output),
        _ => bail!(
            "usage: psychopomp [output] | psychopomp plan <command> | {}",
            plan_runtime::verify::USAGE
        ),
    };

    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("create output directory {}", parent.display()))?;
    }

    pollster::block_on(plan_runtime::render_builtin_hero(&output))
}

#[cfg(test)]
mod wgsl_tests {
    use wgpu::naga::{
        front::wgsl,
        valid::{Capabilities, ValidationFlags, Validator},
    };

    fn validate_wgsl(
        label: &str,
        source: &str,
        expected_entries: &[&str],
        expected_structs: &[(&str, u32)],
    ) {
        let module = wgsl::parse_str(source).unwrap_or_else(|err| {
            panic!("{label}: WGSL parse error:\n{}", err.emit_to_string(source))
        });
        let mut validator = Validator::new(ValidationFlags::all(), Capabilities::default());
        validator
            .validate(&module)
            .unwrap_or_else(|err| panic!("{label}: WGSL validation error: {err:?}"));
        for &entry in expected_entries {
            assert!(
                module.entry_points.iter().any(|ep| ep.name == entry),
                "{label}: missing entry point '{entry}'"
            );
        }
        for &(struct_name, expected_span) in expected_structs {
            let (_, ty) = module
                .types
                .iter()
                .find(|(_, ty)| ty.name.as_deref() == Some(struct_name))
                .unwrap_or_else(|| panic!("{label}: missing struct '{struct_name}'"));
            let wgpu::naga::TypeInner::Struct { span, .. } = ty.inner else {
                panic!("{label}: '{struct_name}' is not a struct");
            };
            assert_eq!(
                span, expected_span,
                "{label}: struct '{struct_name}' byte size mismatch with Rust #[repr(C)] layout"
            );
        }
    }

    #[test]
    fn all_shader_modules_and_composed_effect_pipelines_validate_without_a_gpu() {
        validate_wgsl(
            "scene.wgsl",
            include_str!("scene.wgsl"),
            &["vertex_main", "fragment_main"],
            &[("SceneUniforms", 80)],
        );
        validate_wgsl(
            "present.wgsl",
            include_str!("plan_runtime/presentation/present.wgsl"),
            &["vertex_main", "fragment_main"],
            &[],
        );
        validate_wgsl(
            "grid.wgsl",
            include_str!("render/grid.wgsl"),
            &["vertex_main", "fragment_main", "fragment_heading"],
            &[("Camera", 80)],
        );
        validate_wgsl(
            "grid/edges.wgsl",
            include_str!("render/grid/edges.wgsl"),
            &["vertex_main", "fragment_depth", "fragment_ink"],
            &[("Camera", 80)],
        );
        validate_wgsl(
            "grid/edges_composite.wgsl",
            include_str!("render/grid/edges_composite.wgsl"),
            &["vertex_main", "fragment_main"],
            &[],
        );
        let joined = |modules: &[(&str, &str)]| {
            modules
                .iter()
                .map(|(_, source)| *source)
                .collect::<Vec<_>>()
                .join("\n")
        };
        let size = |name: &str| {
            crate::render::SHADER_STRUCTS
                .iter()
                .find(|(struct_name, _)| *struct_name == name)
                .map(|&(_, bytes)| bytes as u32)
                .unwrap()
        };
        validate_wgsl(
            "stage primitives",
            &joined(&crate::render::PRIMITIVE_SHADER),
            &["vs", "fs"],
            &[("Globals", size("Globals")), ("Prim", size("Prim"))],
        );
        validate_wgsl(
            "stage post",
            &joined(&crate::render::POST_SHADER),
            &["vs", "accumulate", "prefilter", "down", "up", "composite"],
            &[("Post", size("Post"))],
        );
        validate_wgsl(
            "exposure_gpu.wgsl",
            include_str!("exposure_gpu.wgsl"),
            &[
                "vs",
                "fs_add",
                "fs_resolve_opaque",
                "fs_resolve_alpha",
                "fs_composite_over",
            ],
            &[(
                "ShutterUniform",
                std::mem::size_of::<crate::exposure_gpu::ShutterUniform>() as u32,
            )],
        );
    }
}
