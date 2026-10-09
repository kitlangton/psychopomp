//! Optional native inspection overlay; never part of authored/video rendering.
use super::*;

impl HeadlessRenderer {
    pub(crate) fn composite_debug_hud(&mut self, pixels: &mut [u8], lines: &[String]) {
        let count = lines.len().min(8);
        let p = self.theme.palette();
        {
            let mut canvas = ui::card::UiCanvas::new(pixels, [self.spec.width, self.spec.height]);
            canvas.surface(
                ui::Bounds {
                    origin: [20., 16.],
                    size: [self.spec.width as f32 - 40., count as f32 * 26. + 20.],
                },
                ui::card::SurfaceStyle::new(
                    ui::card::Fill::Solid(ui::card::UiColor::srgb8(
                        p.surface[0],
                        p.surface[1],
                        p.surface[2],
                        246,
                    )),
                    8.,
                )
                .border(
                    1.,
                    ui::card::UiColor::srgb8(p.accent[0], p.accent[1], p.accent[2], 110),
                    1.,
                ),
                1.,
            );
        }
        for (index, line) in lines.iter().take(count).enumerate() {
            let text = line.chars().take(160).collect::<String>();
            let mut hasher = DefaultHasher::new();
            text.hash(&mut hasher);
            p.text.hash(&mut hasher);
            let fingerprint = hasher.finish();
            // One reusable slot per HUD row, not a new cache entry per time.
            let key = format!("native-debug:{index}");
            let sprite = refresh_sprite(&mut self.part_sprites, &*key, fingerprint, || {
                let attrs = Attrs::new()
                    .family(fonts::mono())
                    .color(Color::rgb(p.text[0], p.text[1], p.text[2]));
                make_sprite(
                    &mut self.font_system,
                    &mut self.swash_cache,
                    vec![(text.as_str(), attrs.clone())],
                    attrs,
                    Metrics::new(18., 26.),
                    self.spec.width.saturating_sub(72),
                    26,
                )
            });
            composite_text(
                pixels,
                [self.spec.width, self.spec.height],
                TextDraw::new(sprite, [36., 26. + index as f32 * 26.]),
            );
        }
    }
}
