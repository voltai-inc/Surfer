use crate::message::Message;
use crate::view::{DrawConfig, DrawingContext};
use crate::{wave_data::WaveData, SystemState};
use egui::{Context, Frame, PointerButton, Sense, TopBottomPanel, Ui};
use emath::{Align2, Pos2, Rect, RectTransform, Vec2};
use epaint::CornerRadiusF32;

impl SystemState {
    pub fn add_overview_panel(&self, ctx: &Context, waves: &WaveData, msgs: &mut Vec<Message>) {
        TopBottomPanel::bottom("overview")
            .frame(Frame {
                fill: self.user.config.theme.primary_ui_color.background,
                ..Default::default()
            })
            .show(ctx, |ui| {
                self.draw_overview(ui, waves, msgs);
            });
    }

    fn draw_overview(&self, ui: &mut Ui, waves: &WaveData, msgs: &mut Vec<Message>) {
        let (response, mut painter) = ui.allocate_painter(ui.available_size(), Sense::drag());
        let frame_width = response.rect.width();
        let frame_height = response.rect.height();
        let cfg = DrawConfig::new(
            frame_height,
            self.user.config.layout.waveforms_line_height,
            self.user.config.layout.waveforms_text_size,
        );
        let container_rect = Rect::from_min_size(Pos2::ZERO, response.rect.size());
        let to_screen = RectTransform::from_to(container_rect, response.rect);

        let mut ctx = DrawingContext {
            painter: &mut painter,
            cfg: &cfg,
            // This 0.5 is very odd, but it fixes the lines we draw being smushed out across two
            // pixels, resulting in dimmer colors https://github.com/emilk/egui/issues/1322
            // 1 comes from subtracting .5 in cursor draw as y-adjusement is not required for known vertical lines.
            to_screen: &|x, y| to_screen.transform_pos(Pos2::new(x, y) + Vec2::new(0.5, 1.)),
            theme: &self.user.config.theme,
        };

        let num_timestamps = waves.num_timestamps().unwrap_or(1.into());
        let viewport_all = waves.viewport_all();
        for vidx in 0..waves.viewports.len() {
            let minx = viewport_all.pixel_from_absolute_time(
                waves.viewports[vidx].curr_left.absolute(&num_timestamps),
                frame_width,
                &num_timestamps,
            );
            let maxx = viewport_all.pixel_from_absolute_time(
                waves.viewports[vidx].curr_right.absolute(&num_timestamps),
                frame_width,
                &num_timestamps,
            );
            let min = (ctx.to_screen)(minx, 0.);
            let max = (ctx.to_screen)(maxx, container_rect.max.y);
            ctx.painter.rect_filled(
                Rect { min, max },
                CornerRadiusF32::ZERO,
                self.user
                    .config
                    .theme
                    .canvas_colors
                    .foreground
                    .gamma_multiply(0.3),
            );
        }

        waves.draw_cursor(
            &self.user.config.theme,
            &mut ctx,
            response.rect.size(),
            &viewport_all,
        );

        let mut ticks = waves.get_ticks(
            &viewport_all,
            &waves.inner.metadata().timescale,
            frame_width,
            cfg.text_size,
            &self.user.wanted_timeunit,
            &self.get_time_format(),
            &self.user.config,
        );

        if ticks.len() >= 2 {
            ticks.pop();
            ticks.remove(0);
            waves.draw_ticks(
                None,
                &ticks,
                &ctx,
                frame_height * 0.5,
                Align2::CENTER_CENTER,
                &self.user.config,
            );
        }

        waves.draw_markers(
            &self.user.config.theme,
            &mut ctx,
            response.rect.size(),
            &viewport_all,
        );

        waves.draw_marker_number_boxes(
            &mut ctx,
            response.rect.size(),
            &self.user.config.theme,
            &viewport_all,
        );
        response.dragged_by(PointerButton::Primary).then(|| {
            let pointer_pos_global = ui.input(|i| i.pointer.interact_pos());
            let pos = pointer_pos_global.map(|p| to_screen.inverse().transform_pos(p));
            if let Some(pos) = pos {
                let timestamp = viewport_all.as_time_bigint(pos.x, frame_width, &num_timestamps);
                msgs.push(Message::GoToTime(Some(timestamp), 0));
            }
        });
    }
}
