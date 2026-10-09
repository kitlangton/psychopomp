//! Row-band parallelism for CPU rasterisation.

use std::sync::Mutex;

const BYTES_PER_PIXEL: usize = 4;
/// The fewest pixels worth a thread of their own: 32 full-width 1080p rows,
/// so the editor's card and text bands stay parallel while a tall, narrow
/// fill stays on the calling thread instead of paying for spawns.
const PIXELS_PER_WORKER: usize = 32 * 1920;

/// The machine's available parallelism, asked once.
pub(crate) fn available_workers() -> usize {
    static WORKERS: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
    *WORKERS.get_or_init(|| std::thread::available_parallelism().map_or(1, usize::from))
}

/// Paint rows `min_y..=max_y` (clamped to the destination) one row at a
/// time, across threads when the area, rows times the `columns` each row
/// touches, is worth it.
pub(crate) fn for_each_row_band(
    destination: &mut [u8],
    destination_size: [u32; 2],
    min_y: i32,
    max_y: i32,
    columns: i32,
    paint_row: impl Fn(i32, &mut [u8]) + Sync,
) {
    let row_bytes = destination_size[0] as usize * BYTES_PER_PIXEL;
    for_each_band(
        destination,
        destination_size,
        min_y,
        max_y,
        columns,
        |band_start_y, band| {
            for (offset, row) in band.chunks_exact_mut(row_bytes).enumerate() {
                paint_row(band_start_y + offset as i32, row);
            }
        },
    );
}

/// Paint rows `min_y..=max_y` in bands; see [`for_each_row_band`].
pub(crate) fn for_each_band(
    destination: &mut [u8],
    destination_size: [u32; 2],
    min_y: i32,
    max_y: i32,
    columns: i32,
    paint_band: impl Fn(i32, &mut [u8]) + Sync,
) {
    let start_y = min_y.max(0) as usize;
    let end_y = (max_y.saturating_add(1)).clamp(0, destination_size[1] as i32) as usize;
    if start_y >= end_y {
        return;
    }
    let row_bytes = destination_size[0] as usize * BYTES_PER_PIXEL;
    let rows = &mut destination[start_y * row_bytes..end_y * row_bytes];
    let row_count = end_y - start_y;
    let workers = band_workers(row_count, columns, destination_size[0], available_workers());
    if workers <= 1 {
        paint_band(start_y as i32, rows);
        return;
    }
    let rows_per_band = row_count.div_ceil(workers * 8).max(4);
    let bands = Mutex::new(rows.chunks_mut(rows_per_band * row_bytes).enumerate());
    let paint_bands = || {
        loop {
            let Some((band_index, band)) = bands.lock().map_or(None, |mut bands| bands.next())
            else {
                return;
            };
            paint_band((start_y + band_index * rows_per_band) as i32, band);
        }
    };
    std::thread::scope(|scope| {
        for _ in 1..workers {
            scope.spawn(paint_bands);
        }
        paint_bands();
    });
}

/// Threads for `rows` rows of `columns` pixels each (clamped to the width).
fn band_workers(rows: usize, columns: i32, width: u32, available: usize) -> usize {
    let columns = columns.clamp(0, width.min(i32::MAX as u32) as i32) as usize;
    available.min(rows * columns / PIXELS_PER_WORKER).max(1)
}

#[cfg(test)]
mod tests {
    use super::{band_workers, for_each_band, for_each_row_band};

    #[test]
    fn bands_split_by_area_not_rows() {
        // A tall, narrow fill stays serial; the editor's card and code bands do not.
        assert_eq!(band_workers(1080, 8, 1920, 16), 1);
        assert_eq!(band_workers(1080, -5, 1920, 16), 1);
        assert_eq!(band_workers(40, 1920, 1920, 16), 1);
        assert!(band_workers(760, 1574, 1920, 16) > 8);
        assert_eq!(band_workers(1080, 1920, 1920, 16), 16);
        assert_eq!(band_workers(1080, 10_000, 1920, 4), 4);
    }

    #[test]
    fn wide_row_bands_match_a_serial_loop() {
        let size = [1920_u32, 300];
        let paint = |y: i32, row: &mut [u8]| {
            for (x, pixel) in row.chunks_exact_mut(4).enumerate() {
                pixel[0] = pixel[0].wrapping_add(1);
                pixel[1] = y as u8;
                pixel[2] = x as u8;
            }
        };
        let mut banded = vec![0_u8; 1920 * 300 * 4];
        for_each_row_band(&mut banded, size, 3, 290, 1920, paint);
        let mut serial = vec![0_u8; 1920 * 300 * 4];
        for (y, row) in serial
            .chunks_exact_mut(1920 * 4)
            .enumerate()
            .take(291)
            .skip(3)
        {
            paint(y as i32, row);
        }
        assert_eq!(banded, serial);
    }

    #[test]
    fn row_bands_paint_each_requested_row_once_like_a_serial_loop() {
        let size = [7_u32, 300];
        for (min_y, max_y) in [(0, 299), (13, 250), (-5, 400), (40, 40), (90, 10)] {
            let mut banded = vec![0_u8; 7 * 300 * 4];
            for_each_row_band(&mut banded, size, min_y, max_y, 7, |y, row| {
                for (x, pixel) in row.chunks_exact_mut(4).enumerate() {
                    pixel[0] = pixel[0].wrapping_add(1);
                    pixel[1] = y as u8;
                    pixel[2] = x as u8;
                }
            });
            let mut serial = vec![0_u8; 7 * 300 * 4];
            for y in min_y.max(0)..=max_y.min(299) {
                for x in 0..7 {
                    let index = (y as usize * 7 + x) * 4;
                    serial[index..index + 3].copy_from_slice(&[1, y as u8, x as u8]);
                }
            }
            assert_eq!(banded, serial, "rows {min_y}..={max_y}");
        }
    }

    #[test]
    fn bands_cover_each_requested_row_once_starting_at_their_first_row() {
        let size = [3_u32, 1000];
        let mut seen = vec![0_u8; 3 * 1000 * 4];
        for_each_band(&mut seen, size, 10, 989, 3, |start_y, band| {
            for (offset, row) in band.chunks_exact_mut(12).enumerate() {
                row[0] += 1;
                row[1] = (start_y as usize + offset) as u8;
            }
        });
        for y in 0..1000 {
            let row = &seen[y * 12..y * 12 + 2];
            let expected = if (10..=989).contains(&y) {
                [1, y as u8]
            } else {
                [0, 0]
            };
            assert_eq!(row, expected, "row {y}");
        }
    }
}
