// =============================================================================
//        #######
//     ###       ###     F: diagnostics.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/15 08:11:39 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/15 10:24:40 by dnettoRaw
//      ###########      S: 0.1.0-beta.2
// =============================================================================

use appcore_render::NativeRenderer;
use appcore_ui::{
    elements, native_process_resident_bytes, UiNativeNodeV1, UiNativeWindow, UiRectangle,
};
use std::time::{Duration, Instant};
pub const OVERLAY: u32 = 210;
pub fn nodes(nodes: &mut Vec<UiNativeNodeV1>) {
    for (id,y,label) in [("perf",108.,"FPS · frame time · CPU submit"),("memory",132.,"RAM (RSS) · GPU payload · resolution"),("gpu",156.,"GPU / queue"),("navigation",180.,"Click scene · drag: orbit · W/S: zoom · A/D/←→: orbit · ↑↓: tilt · wheel: zoom · Esc: release")] {
        nodes.push(elements::text(id,UiRectangle{x:20.,y,width:984.,height:22.},label));
    }
}
pub struct Hud {
    samples: [f64; 128],
    n: usize,
    cursor: usize,
    previous: Option<Instant>,
    period: Instant,
    frames: u64,
    submit_ms: f64,
    focus: bool,
}
impl Default for Hud {
    fn default() -> Self {
        Self {
            samples: [0.; 128],
            n: 0,
            cursor: 0,
            previous: None,
            period: Instant::now(),
            frames: 0,
            submit_ms: 0.,
            focus: false,
        }
    }
}
impl Hud {
    pub fn navigation(
        &mut self,
        window: &UiNativeWindow,
        active: bool,
    ) -> appcore_ui::UiResult<()> {
        if self.focus != active {
            window.set_label("navigation", if active {
                "CAMERA ACTIVE · drag: orbit · W/S: zoom · A/D/←→: orbit · ↑↓: tilt · wheel: zoom · Esc: release"
            } else {
                "Click scene to navigate · drag: orbit · W/S: zoom · A/D/←→: orbit · ↑↓: tilt · wheel: zoom"
            })?;
            self.focus = active;
        }
        Ok(())
    }
    pub fn presented(&mut self, submit: Duration) {
        let now = Instant::now();
        if let Some(previous) = self.previous {
            self.samples[self.cursor] = (now - previous).as_secs_f64() * 1000.;
            self.cursor = (self.cursor + 1) % 128;
            self.n = (self.n + 1).min(128);
        }
        self.previous = Some(now);
        self.frames = self.frames.saturating_add(1);
        self.submit_ms = submit.as_secs_f64() * 1000.;
    }
    pub fn update(
        &mut self,
        window: &UiNativeWindow,
        renderer: &NativeRenderer<'_>,
        size: (u32, u32),
    ) -> appcore_ui::UiResult<()> {
        if self.period.elapsed() < Duration::from_millis(500) {
            return Ok(());
        }
        let fps = self.frames as f64 / self.period.elapsed().as_secs_f64();
        let (mean, p95) = stats(&self.samples[..self.n]);
        window.set_label("perf",&format!("FPS (present) {fps:.1}   |   frame {mean:.2} ms   |   p95 {p95:.2} ms   |   CPU submit {:.2} ms",self.submit_ms))?;
        let ram = native_process_resident_bytes()
            .map(|bytes| format!("{:.1} MiB", bytes as f64 / 1048576.))
            .unwrap_or_else(|_| "unavailable".into());
        window.set_label(
            "memory",
            &format!(
                "RAM (process RSS) {ram}   |   GPU payload {:.1} MiB   |   {} × {} physical pixels",
                renderer.managed_gpu_bytes() as f64 / 1048576.,
                size.0,
                size.1
            ),
        )?;
        let info = renderer.adapter_info();
        let name: String = info
            .name
            .chars()
            .filter(|c| !c.is_control())
            .take(48)
            .collect();
        let progress = renderer.frame_progress();
        window.set_label(
            "gpu",
            &format!(
                "{:?} · {name}   |   GPU pending {}/2   |   submitted {} / completed {}",
                info.backend,
                renderer.inflight(),
                progress.submitted,
                progress.completed
            ),
        )?;
        self.period = Instant::now();
        self.frames = 0;
        Ok(())
    }
}
fn stats(samples: &[f64]) -> (f64, f64) {
    if samples.is_empty() {
        return (0., 0.);
    }
    let mut sorted = [0.; 128];
    sorted[..samples.len()].copy_from_slice(samples);
    sorted[..samples.len()].sort_by(f64::total_cmp);
    (
        samples.iter().sum::<f64>() / samples.len() as f64,
        sorted[(samples.len() * 95).div_ceil(100) - 1],
    )
}
pub fn deadline() -> Result<Option<Instant>, Box<dyn std::error::Error>> {
    match std::env::var("E3DN_DEMO_SECONDS") {
        Err(std::env::VarError::NotPresent) => Ok(None),
        Ok(value) => {
            let seconds: u64 = value.parse()?;
            if !(1..=600).contains(&seconds) {
                return Err("E3DN_DEMO_SECONDS must be 1..600".into());
            }
            Ok(Some(Instant::now() + Duration::from_secs(seconds)))
        }
        Err(error) => Err(error.into()),
    }
}
#[cfg(test)]
mod tests {
    #[test]
    fn frame_percentiles_use_nearest_rank_and_bound_storage() {
        assert_eq!(super::stats(&[]), (0., 0.));
        assert_eq!(super::stats(&[3., 1., 2.]), (2., 3.));
        let mut hud = super::Hud::default();
        for _ in 0..300 {
            hud.presented(std::time::Duration::from_millis(1));
        }
        assert_eq!(hud.n, 128);
    }
}
