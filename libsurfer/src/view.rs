use std::ops::Range;

use crate::fzcmd::expand_command;
use ecolor::Color32;
#[cfg(not(target_arch = "wasm32"))]
use egui::ViewportCommand;
use egui::{
    FontId, FontSelection, Frame, Layout, Painter, RichText, ScrollArea, Sense, TextFormat,
    TextStyle, UiBuilder, WidgetText,
};
use egui_extras::{Column, TableBuilder};
use egui_remixicon::icons;
use emath::{Align, GuiRounding, Pos2, Rect, RectTransform, Vec2};
use epaint::{
    text::{LayoutJob, TextWrapMode},
    CornerRadiusF32, Margin, Stroke,
};
use eyre::Context;
use itertools::Itertools;
use log::{info, warn};

use num::BigUint;
use surfer_translation_types::{
    translator::{TrueName, VariableNameInfo},
    SubFieldFlatTranslationResult, TranslatedValue, Translator, VariableInfo, VariableType,
};

#[cfg(feature = "performance_plot")]
use crate::benchmark::NUM_PERF_SAMPLES;
use crate::command_parser::get_parser;
use crate::displayed_item::{
    draw_rename_window, DisplayedFieldRef, DisplayedItem, DisplayedItemRef,
};
use crate::displayed_item_tree::{ItemIndex, VisibleItemIndex};
use crate::help::{
    draw_about_window, draw_control_help_window, draw_license_window, draw_quickstart_help_window,
};
use crate::time::time_string;
use crate::transaction_container::{StreamScopeRef, TransactionStreamRef};
use crate::translation::TranslationResultExt;
use crate::util::uint_idx_to_alpha_idx;
use crate::variable_direction::VariableDirectionExt;
use crate::variable_filter::VariableFilter;
use crate::wave_container::{
    FieldRef, FieldRefExt, ScopeRef, ScopeRefExt, VariableRef, VariableRefExt, WaveContainer,
};
use crate::wave_data::ScopeType;
use crate::{
    command_prompt::show_command_prompt, hierarchy, hierarchy::HierarchyStyle, wave_data::WaveData,
    Message, MoveDir, SystemState,
};
use crate::{config::SurferTheme, wave_container::VariableMeta};
use crate::{data_container::VariableType as VarType, OUTSTANDING_TRANSACTIONS};

pub struct DrawingContext<'a> {
    pub painter: &'a mut Painter,
    pub cfg: &'a DrawConfig,
    pub to_screen: &'a dyn Fn(f32, f32) -> Pos2,
    pub theme: &'a SurferTheme,
}

#[derive(Debug)]
pub struct DrawConfig {
    pub canvas_height: f32,
    pub line_height: f32,
    pub text_size: f32,
    pub extra_draw_width: i32,
}

impl DrawConfig {
    pub fn new(canvas_height: f32, line_height: f32, text_size: f32) -> Self {
        Self {
            canvas_height,
            line_height,
            text_size,
            extra_draw_width: 6,
        }
    }
}

#[derive(Debug)]
pub struct VariableDrawingInfo {
    pub field_ref: FieldRef,
    pub displayed_field_ref: DisplayedFieldRef,
    pub item_list_idx: VisibleItemIndex,
    pub top: f32,
    pub bottom: f32,
}

#[derive(Debug)]
pub struct DividerDrawingInfo {
    pub item_list_idx: VisibleItemIndex,
    pub top: f32,
    pub bottom: f32,
}

#[derive(Debug)]
pub struct MarkerDrawingInfo {
    pub item_list_idx: VisibleItemIndex,
    pub top: f32,
    pub bottom: f32,
    pub idx: u8,
}

#[derive(Debug)]
pub struct TimeLineDrawingInfo {
    pub item_list_idx: VisibleItemIndex,
    pub top: f32,
    pub bottom: f32,
}

#[derive(Debug)]
pub struct StreamDrawingInfo {
    pub transaction_stream_ref: TransactionStreamRef,
    pub item_list_idx: VisibleItemIndex,
    pub top: f32,
    pub bottom: f32,
}

#[derive(Debug)]
pub struct GroupDrawingInfo {
    pub item_list_idx: VisibleItemIndex,
    pub top: f32,
    pub bottom: f32,
}

pub enum ItemDrawingInfo {
    Variable(VariableDrawingInfo),
    Divider(DividerDrawingInfo),
    Marker(MarkerDrawingInfo),
    TimeLine(TimeLineDrawingInfo),
    Stream(StreamDrawingInfo),
    Group(GroupDrawingInfo),
}

impl ItemDrawingInfo {
    pub fn top(&self) -> f32 {
        match self {
            ItemDrawingInfo::Variable(drawing_info) => drawing_info.top,
            ItemDrawingInfo::Divider(drawing_info) => drawing_info.top,
            ItemDrawingInfo::Marker(drawing_info) => drawing_info.top,
            ItemDrawingInfo::TimeLine(drawing_info) => drawing_info.top,
            ItemDrawingInfo::Stream(drawing_info) => drawing_info.top,
            ItemDrawingInfo::Group(drawing_info) => drawing_info.top,
        }
    }
    pub fn bottom(&self) -> f32 {
        match self {
            ItemDrawingInfo::Variable(drawing_info) => drawing_info.bottom,
            ItemDrawingInfo::Divider(drawing_info) => drawing_info.bottom,
            ItemDrawingInfo::Marker(drawing_info) => drawing_info.bottom,
            ItemDrawingInfo::TimeLine(drawing_info) => drawing_info.bottom,
            ItemDrawingInfo::Stream(drawing_info) => drawing_info.bottom,
            ItemDrawingInfo::Group(drawing_info) => drawing_info.bottom,
        }
    }
    pub fn item_list_idx(&self) -> VisibleItemIndex {
        match self {
            ItemDrawingInfo::Variable(drawing_info) => drawing_info.item_list_idx,
            ItemDrawingInfo::Divider(drawing_info) => drawing_info.item_list_idx,
            ItemDrawingInfo::Marker(drawing_info) => drawing_info.item_list_idx,
            ItemDrawingInfo::TimeLine(drawing_info) => drawing_info.item_list_idx,
            ItemDrawingInfo::Stream(drawing_info) => drawing_info.item_list_idx,
            ItemDrawingInfo::Group(drawing_info) => drawing_info.item_list_idx,
        }
    }
}

impl eframe::App for SystemState {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        #[cfg(feature = "performance_plot")]
        self.timing.borrow_mut().start_frame();

        if self.continuous_redraw {
            self.invalidate_draw_commands();
        }

        let (fullscreen, window_size) = ctx.input(|i| {
            (
                i.viewport().fullscreen.unwrap_or_default(),
                Some(i.screen_rect.size()),
            )
        });
        #[cfg(target_arch = "wasm32")]
        let _ = fullscreen;

        #[cfg(feature = "performance_plot")]
        self.timing.borrow_mut().start("draw");
        let mut msgs = self.draw(ctx, window_size);
        #[cfg(feature = "performance_plot")]
        self.timing.borrow_mut().end("draw");

        #[cfg(feature = "performance_plot")]
        self.timing.borrow_mut().start("update");
        let ui_zoom_factor = self.ui_zoom_factor();
        if ctx.zoom_factor() != ui_zoom_factor {
            ctx.set_zoom_factor(ui_zoom_factor);
        }

        self.items_to_expand.borrow_mut().clear();

        while let Some(msg) = msgs.pop() {
            #[cfg(not(target_arch = "wasm32"))]
            if let Message::Exit = msg {
                ctx.send_viewport_cmd(ViewportCommand::Close);
            }
            #[cfg(not(target_arch = "wasm32"))]
            if let Message::ToggleFullscreen = msg {
                ctx.send_viewport_cmd(ViewportCommand::Fullscreen(!fullscreen));
            }
            self.update(msg);
        }
        #[cfg(feature = "performance_plot")]
        self.timing.borrow_mut().end("update");

        #[cfg(feature = "performance_plot")]
        self.timing.borrow_mut().start("handle_async_messages");
        #[cfg(feature = "performance_plot")]
        self.timing.borrow_mut().end("handle_async_messages");

        self.handle_async_messages();
        self.handle_batch_commands();
        #[cfg(target_arch = "wasm32")]
        self.handle_wasm_external_messages();

        let viewport_is_moving = if let Some(waves) = &mut self.user.waves {
            let mut is_moving = false;
            for vp in &mut waves.viewports {
                if vp.is_moving() {
                    vp.move_viewport(ctx.input(|i| i.stable_dt));
                    is_moving = true;
                }
            }
            is_moving
        } else {
            false
        };

        if let Some(waves) = self.user.waves.as_ref().and_then(|w| w.inner.as_waves()) {
            waves.tick()
        }

        if viewport_is_moving {
            self.invalidate_draw_commands();
            ctx.request_repaint();
        }

        #[cfg(feature = "performance_plot")]
        self.timing.borrow_mut().start("handle_wcp_commands");
        self.handle_wcp_commands();
        #[cfg(feature = "performance_plot")]
        self.timing.borrow_mut().end("handle_wcp_commands");

        // We can save some user battery life by not redrawing unless needed. At the moment,
        // we only need to continuously redraw to make surfer interactive during loading, otherwise
        // we'll let egui manage repainting. In practice
        if self.continuous_redraw
            || self.progress_tracker.is_some()
            || self.user.show_performance
            || OUTSTANDING_TRANSACTIONS.load(std::sync::atomic::Ordering::SeqCst) != 0
        {
            ctx.request_repaint();
        }

        #[cfg(feature = "performance_plot")]
        if let Some(prev_cpu) = frame.info().cpu_usage {
            self.rendering_cpu_times.push_back(prev_cpu);
            if self.rendering_cpu_times.len() > NUM_PERF_SAMPLES {
                self.rendering_cpu_times.pop_front();
            }
        }

        #[cfg(feature = "performance_plot")]
        self.timing.borrow_mut().end_frame();
    }
}

impl SystemState {
    pub(crate) fn draw(&mut self, ctx: &egui::Context, window_size: Option<Vec2>) -> Vec<Message> {
        let max_width = ctx.available_rect().width();
        let max_height = ctx.available_rect().height();

        let mut msgs = vec![];

        if self.user.show_about {
            draw_about_window(ctx, &mut msgs);
        }

        if self.user.show_license {
            draw_license_window(ctx, &mut msgs);
        }

        if self.user.show_keys {
            draw_control_help_window(ctx, &mut msgs);
        }

        if self.user.show_quick_start {
            draw_quickstart_help_window(ctx, &mut msgs);
        }

        if self.user.show_gestures {
            self.mouse_gesture_help(ctx, &mut msgs);
        }

        if self.user.show_logs {
            self.draw_log_window(ctx, &mut msgs);
        }

        if let Some(dialog) = &self.user.show_reload_suggestion {
            self.draw_reload_waveform_dialog(ctx, dialog, &mut msgs);
        }

        if let Some(dialog) = &self.user.show_open_sibling_state_file_suggestion {
            self.draw_open_sibling_state_file_dialog(ctx, dialog, &mut msgs);
        }

        if self.user.show_performance {
            #[cfg(feature = "performance_plot")]
            self.draw_performance_graph(ctx, &mut msgs);
        }

        if self.user.show_cursor_window {
            if let Some(waves) = &self.user.waves {
                self.draw_marker_window(waves, ctx, &mut msgs);
            }
        }

        if let Some(idx) = self.user.rename_target {
            draw_rename_window(
                ctx,
                &mut msgs,
                idx,
                &mut self.item_renaming_string.borrow_mut(),
            );
        }

        if self
            .user
            .show_menu
            .unwrap_or_else(|| self.user.config.layout.show_menu())
        {
            self.add_menu_panel(ctx, &mut msgs);
        }

        if self.show_toolbar() {
            self.add_toolbar_panel(ctx, &mut msgs);
        }

        if self.user.show_url_entry {
            self.draw_load_url(ctx, &mut msgs);
        }

        if self.show_statusbar() {
            self.add_statusbar_panel(ctx, &self.user.waves, &mut msgs);
        }
        if let Some(waves) = &self.user.waves {
            if self.show_overview() && !waves.items_tree.is_empty() {
                self.add_overview_panel(ctx, waves, &mut msgs);
            }
        }

        if self.show_hierarchy() {
            egui::SidePanel::left("variable select left panel")
                .default_width(300.)
                .width_range(100.0..=max_width)
                .frame(Frame {
                    fill: self.user.config.theme.primary_ui_color.background,
                    ..Default::default()
                })
                .show(ctx, |ui| {
                    self.user.sidepanel_width = Some(ui.clip_rect().width());
                    match self.hierarchy_style() {
                        HierarchyStyle::Separate => hierarchy::separate(self, ui, &mut msgs),
                        HierarchyStyle::Tree => hierarchy::tree(self, ui, &mut msgs),
                    }
                });
        }

        if self.command_prompt.visible {
            show_command_prompt(self, ctx, window_size, &mut msgs);
            if let Some(new_idx) = self.command_prompt.new_selection {
                self.command_prompt.selected = new_idx;
                self.command_prompt.new_selection = None;
            }
        }

        if self.user.waves.is_some() {
            let scroll_offset = self.user.waves.as_ref().unwrap().scroll_offset;
            if self.user.waves.as_ref().unwrap().any_displayed() {
                let draw_focus_ids = self.command_prompt.visible
                    && expand_command(&self.command_prompt_text.borrow(), get_parser(self))
                        .expanded
                        .starts_with("item_focus");
                if draw_focus_ids {
                    egui::SidePanel::left("focus id list")
                        .default_width(40.)
                        .width_range(40.0..=max_width)
                        .show(ctx, |ui| {
                            self.handle_pointer_in_ui(ui, &mut msgs);
                            let response = ScrollArea::both()
                                .vertical_scroll_offset(scroll_offset)
                                .show(ui, |ui| {
                                    self.draw_item_focus_list(ui);
                                });
                            self.user.waves.as_mut().unwrap().top_item_draw_offset =
                                response.inner_rect.min.y;
                            self.user.waves.as_mut().unwrap().total_height =
                                response.inner_rect.height();
                            if (scroll_offset - response.state.offset.y).abs() > 5. {
                                msgs.push(Message::SetScrollOffset(response.state.offset.y));
                            }
                        });
                }

                egui::SidePanel::left("variable list")
                    .default_width(100.)
                    .width_range(100.0..=max_width)
                    .show(ctx, |ui| {
                        ui.style_mut().wrap_mode = Some(TextWrapMode::Extend);
                        self.handle_pointer_in_ui(ui, &mut msgs);
                        if self.show_default_timeline() {
                            ui.label(RichText::new("Time").italics());
                        }

                        let response = ScrollArea::both()
                            .auto_shrink([false; 2])
                            .vertical_scroll_offset(scroll_offset)
                            .show(ui, |ui| {
                                self.draw_item_list(&mut msgs, ui, ctx);
                            });
                        self.user.waves.as_mut().unwrap().top_item_draw_offset =
                            response.inner_rect.min.y;
                        self.user.waves.as_mut().unwrap().total_height =
                            response.inner_rect.height();
                        if (scroll_offset - response.state.offset.y).abs() > 5. {
                            msgs.push(Message::SetScrollOffset(response.state.offset.y));
                        }
                    });

                if self
                    .user
                    .waves
                    .as_ref()
                    .unwrap()
                    .focused_transaction
                    .1
                    .is_some()
                {
                    egui::SidePanel::right("Transaction Details")
                        .default_width(330.)
                        .width_range(10.0..=max_width)
                        .show(ctx, |ui| {
                            ui.style_mut().wrap_mode = Some(TextWrapMode::Extend);
                            self.handle_pointer_in_ui(ui, &mut msgs);
                            self.draw_focused_transaction_details(ui);
                        });
                }

                egui::SidePanel::left("variable values")
                    .frame(Frame {
                        inner_margin: Margin::ZERO,
                        outer_margin: Margin::ZERO,
                        ..Default::default()
                    })
                    .default_width(100.)
                    .width_range(10.0..=max_width)
                    .show(ctx, |ui| {
                        ui.style_mut().wrap_mode = Some(TextWrapMode::Extend);
                        self.handle_pointer_in_ui(ui, &mut msgs);
                        let response = ScrollArea::both()
                            .vertical_scroll_offset(scroll_offset)
                            .show(ui, |ui| self.draw_var_values(ui, &mut msgs));
                        if (scroll_offset - response.state.offset.y).abs() > 5. {
                            msgs.push(Message::SetScrollOffset(response.state.offset.y));
                        }
                    });
                let std_stroke = ctx.style().visuals.widgets.noninteractive.bg_stroke;
                ctx.style_mut(|style| {
                    style.visuals.widgets.noninteractive.bg_stroke = Stroke {
                        width: self.user.config.theme.viewport_separator.width,
                        color: self.user.config.theme.viewport_separator.color,
                    };
                });
                let number_of_viewports = self.user.waves.as_ref().unwrap().viewports.len();
                if number_of_viewports > 1 {
                    // Draw additional viewports
                    let max_width = ctx.available_rect().width();
                    let default_width = max_width / (number_of_viewports as f32);
                    for viewport_idx in 1..number_of_viewports {
                        egui::SidePanel::right(format! {"view port {viewport_idx}"})
                            .default_width(default_width)
                            .width_range(30.0..=max_width)
                            .frame(Frame {
                                inner_margin: Margin::ZERO,
                                outer_margin: Margin::ZERO,
                                ..Default::default()
                            })
                            .show(ctx, |ui| self.draw_items(ctx, &mut msgs, ui, viewport_idx));
                    }
                }

                egui::CentralPanel::default()
                    .frame(Frame {
                        inner_margin: Margin::ZERO,
                        outer_margin: Margin::ZERO,
                        ..Default::default()
                    })
                    .show(ctx, |ui| {
                        self.draw_items(ctx, &mut msgs, ui, 0);
                    });
                ctx.style_mut(|style| {
                    style.visuals.widgets.noninteractive.bg_stroke = std_stroke;
                });
            }
        };

        if self.user.waves.is_none()
            || self
                .user
                .waves
                .as_ref()
                .is_some_and(|waves| !waves.any_displayed())
        {
            egui::CentralPanel::default()
                .frame(Frame::NONE.fill(self.user.config.theme.canvas_colors.background))
                .show(ctx, |ui| {
                    ui.add_space(max_height * 0.1);
                    ui.vertical_centered(|ui| {
                        ui.label(RichText::new("🏄 Surfer").monospace().size(24.));
                        ui.add_space(20.);
                        let layout = Layout::top_down(Align::LEFT);
                        ui.allocate_ui_with_layout(
                            Vec2 {
                                x: max_width * 0.35,
                                y: max_height * 0.5,
                            },
                            layout,
                            |ui| self.help_message(ui),
                        );
                    });
                });
        }

        ctx.input(|i| {
            i.raw.dropped_files.iter().for_each(|file| {
                info!("Got dropped file");
                msgs.push(Message::FileDropped(file.clone()));
            });
        });

        // If some dialogs are open, skip decoding keypresses
        if !self.user.show_url_entry
            && self.user.rename_target.is_none()
            && self.user.show_reload_suggestion.is_none()
        {
            self.handle_pressed_keys(ctx, &mut msgs);
        }
        msgs
    }

    fn draw_load_url(&self, ctx: &egui::Context, msgs: &mut Vec<Message>) {
        let mut open = true;
        egui::Window::new("Load URL")
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    let url = &mut *self.url.borrow_mut();
                    let response = ui.text_edit_singleline(url);
                    ui.horizontal(|ui| {
                        if ui.button("Load URL").clicked()
                            || (response.lost_focus()
                                && ui.input(|i| i.key_pressed(egui::Key::Enter)))
                        {
                            if let Some(callback) = &self.url_callback {
                                msgs.push(callback(url.clone()));
                            }
                            msgs.push(Message::SetUrlEntryVisible(false, None));
                        }
                        if ui.button("Cancel").clicked() {
                            msgs.push(Message::SetUrlEntryVisible(false, None));
                        }
                    });
                });
            });
        if !open {
            msgs.push(Message::SetUrlEntryVisible(false, None));
        }
    }

    fn handle_pointer_in_ui(&self, ui: &mut egui::Ui, msgs: &mut Vec<Message>) {
        if ui.ui_contains_pointer() {
            let scroll_delta = ui.input(|i| i.smooth_scroll_delta);
            if scroll_delta.y > 0.0 {
                msgs.push(Message::InvalidateCount);
                msgs.push(Message::VerticalScroll(MoveDir::Up, self.get_count()));
            } else if scroll_delta.y < 0.0 {
                msgs.push(Message::InvalidateCount);
                msgs.push(Message::VerticalScroll(MoveDir::Down, self.get_count()));
            }
        }
    }

    pub fn draw_all_scopes(
        &self,
        msgs: &mut Vec<Message>,
        wave: &WaveData,
        draw_variables: bool,
        ui: &mut egui::Ui,
        filter: &VariableFilter,
    ) {
        for scope in wave.inner.root_scopes() {
            match scope {
                ScopeType::WaveScope(scope) => {
                    self.draw_selectable_child_or_orphan_scope(
                        msgs,
                        wave,
                        &scope,
                        draw_variables,
                        ui,
                        filter,
                    );
                }
                ScopeType::StreamScope(_) => {
                    self.draw_transaction_root(msgs, wave, ui);
                }
            }
        }
        if draw_variables {
            if let Some(wave_container) = wave.inner.as_waves() {
                let scope = ScopeRef::empty();
                let variables = wave_container.variables_in_scope(&scope);
                self.draw_variable_list(msgs, wave_container, ui, &variables, None, filter);
            }
        }
    }

    fn add_scope_selectable_label(
        &self,
        msgs: &mut Vec<Message>,
        wave: &WaveData,
        scope: &ScopeRef,
        ui: &mut egui::Ui,
    ) {
        let name = scope.name();
        let mut response = ui.add(egui::SelectableLabel::new(
            wave.active_scope == Some(ScopeType::WaveScope(scope.clone())),
            name,
        ));
        let _ = response.interact(egui::Sense::click_and_drag());
        response.drag_started().then(|| {
            msgs.push(Message::VariableDragStarted(VisibleItemIndex(
                self.user.waves.as_ref().unwrap().display_item_ref_counter,
            )))
        });

        response.drag_stopped().then(|| {
            if ui.input(|i| i.pointer.hover_pos().unwrap_or_default().x)
                > self.user.sidepanel_width.unwrap_or_default()
            {
                let scope_t = ScopeType::WaveScope(scope.clone());
                let variables = self
                    .user
                    .waves
                    .as_ref()
                    .unwrap()
                    .inner
                    .variables_in_scope(&scope_t)
                    .iter()
                    .filter_map(|var| match var {
                        VarType::Variable(var) => Some(var.clone()),
                        _ => None,
                    })
                    .collect_vec();

                msgs.push(Message::AddDraggedVariables(self.filtered_variables(
                    variables.as_slice(),
                    &self.user.variable_filter,
                )));
            }
        });
        if self.show_scope_tooltip() {
            response = response.on_hover_ui(|ui| {
                ui.set_max_width(ui.spacing().tooltip_width);
                ui.add(egui::Label::new(scope_tooltip_text(wave, scope)));
            });
        }
        response.context_menu(|ui| {
            if ui.button("Add scope").clicked() {
                msgs.push(Message::AddScope(scope.clone(), false));
                ui.close_menu();
            }
            if ui.button("Add scope recursively").clicked() {
                msgs.push(Message::AddScope(scope.clone(), true));
                ui.close_menu();
            }
            if ui.button("Add scope as group").clicked() {
                msgs.push(Message::AddScopeAsGroup(scope.clone(), false));
                ui.close_menu();
            }
            if ui.button("Add scope as group recursively").clicked() {
                msgs.push(Message::AddScopeAsGroup(scope.clone(), true));
                ui.close_menu();
            }
        });
        response
            .clicked()
            .then(|| msgs.push(Message::SetActiveScope(ScopeType::WaveScope(scope.clone()))));
    }

    fn draw_selectable_child_or_orphan_scope(
        &self,
        msgs: &mut Vec<Message>,
        wave: &WaveData,
        scope: &ScopeRef,
        draw_variables: bool,
        ui: &mut egui::Ui,
        filter: &VariableFilter,
    ) {
        let Some(child_scopes) = wave
            .inner
            .as_waves()
            .unwrap()
            .child_scopes(scope)
            .context("Failed to get child scopes")
            .map_err(|e| warn!("{e:#?}"))
            .ok()
        else {
            return;
        };

        let no_variables_in_scope = wave.inner.as_waves().unwrap().no_variables_in_scope(scope);
        if child_scopes.is_empty() && no_variables_in_scope && !self.show_empty_scopes() {
            return;
        }
        if child_scopes.is_empty() && (!draw_variables || no_variables_in_scope) {
            self.add_scope_selectable_label(msgs, wave, scope, ui);
        } else {
            egui::collapsing_header::CollapsingState::load_with_default_open(
                ui.ctx(),
                egui::Id::new(scope),
                false,
            )
            .show_header(ui, |ui| {
                ui.with_layout(
                    Layout::top_down(Align::LEFT).with_cross_justify(true),
                    |ui| {
                        self.add_scope_selectable_label(msgs, wave, scope, ui);
                    },
                );
            })
            .body(|ui| {
                if draw_variables || self.show_parameters_in_scopes() {
                    let wave_container = wave.inner.as_waves().unwrap();
                    let parameters = wave_container.parameters_in_scope(scope);
                    if !parameters.is_empty() {
                        egui::collapsing_header::CollapsingState::load_with_default_open(
                            ui.ctx(),
                            egui::Id::new(&parameters),
                            false,
                        )
                        .show_header(ui, |ui| {
                            ui.with_layout(
                                Layout::top_down(Align::LEFT).with_cross_justify(true),
                                |ui| {
                                    ui.label("Parameters");
                                },
                            );
                        })
                        .body(|ui| {
                            self.draw_variable_list(
                                msgs,
                                wave_container,
                                ui,
                                &parameters,
                                None,
                                filter,
                            );
                        });
                    }
                }
                self.draw_root_scope_view(msgs, wave, scope, draw_variables, ui, filter);
                if draw_variables {
                    let wave_container = wave.inner.as_waves().unwrap();
                    let variables = wave_container.variables_in_scope(scope);
                    self.draw_variable_list(msgs, wave_container, ui, &variables, None, filter);
                }
            });
        }
    }

    fn draw_root_scope_view(
        &self,
        msgs: &mut Vec<Message>,
        wave: &WaveData,
        root_scope: &ScopeRef,
        draw_variables: bool,
        ui: &mut egui::Ui,
        filter: &VariableFilter,
    ) {
        let Some(child_scopes) = wave
            .inner
            .as_waves()
            .unwrap()
            .child_scopes(root_scope)
            .context("Failed to get child scopes")
            .map_err(|e| warn!("{e:#?}"))
            .ok()
        else {
            return;
        };

        let child_scopes_sorted = child_scopes
            .iter()
            .sorted_by(|a, b| numeric_sort::cmp(&a.name(), &b.name()))
            .collect_vec();

        for child_scope in child_scopes_sorted {
            self.draw_selectable_child_or_orphan_scope(
                msgs,
                wave,
                child_scope,
                draw_variables,
                ui,
                filter,
            );
        }
    }

    pub fn draw_variable_list(
        &self,
        msgs: &mut Vec<Message>,
        wave_container: &WaveContainer,
        ui: &mut egui::Ui,
        all_variables: &[VariableRef],
        row_range: Option<Range<usize>>,
        filter: &VariableFilter,
    ) {
        let all_variables = self.filtered_variables(all_variables, filter);
        self.draw_filtered_variable_list(msgs, wave_container, ui, &all_variables, row_range);
    }

    pub fn draw_filtered_variable_list(
        &self,
        msgs: &mut Vec<Message>,
        wave_container: &WaveContainer,
        ui: &mut egui::Ui,
        all_variables: &[VariableRef],
        row_range: Option<Range<usize>>,
    ) {
        let variables = all_variables
            .iter()
            .map(|var| {
                let meta = wave_container.variable_meta(var).ok();
                let name_info = self.get_variable_name_info(wave_container, var);
                (var, meta, name_info)
            })
            .sorted_by_key(|(_, _, name_info)| {
                -name_info
                    .as_ref()
                    .and_then(|info| info.priority)
                    .unwrap_or_default()
            })
            .skip(row_range.as_ref().map(|r| r.start).unwrap_or(0))
            .take(
                row_range
                    .as_ref()
                    .map(|r| r.end - r.start)
                    .unwrap_or(all_variables.len()),
            );

        for (variable, meta, name_info) in variables {
            let index = meta
                .as_ref()
                .and_then(|meta| meta.index)
                .map(|index| {
                    if self.show_variable_indices() {
                        format!(" {index}")
                    } else {
                        String::new()
                    }
                })
                .unwrap_or_default();

            let direction = if self.show_variable_direction() {
                meta.as_ref()
                    .and_then(|meta| meta.direction)
                    .map(|direction| {
                        format!(
                            "{} ",
                            // Icon based on direction
                            direction.get_icon().unwrap_or_else(|| {
                                if meta.as_ref().is_some_and(|meta| {
                                    meta.variable_type == Some(VariableType::VCDParameter)
                                }) {
                                    // If parameter
                                    icons::MAP_PIN_2_LINE
                                } else {
                                    // Align other items (can be improved)
                                    // The padding depends on if we will render monospace or not
                                    if name_info.is_some() {
                                        "  "
                                    } else {
                                        "    "
                                    }
                                }
                            })
                        )
                    })
                    .unwrap_or_default()
            } else {
                String::new()
            };

            let value = if meta
                .as_ref()
                .is_some_and(|meta| meta.variable_type == Some(VariableType::VCDParameter))
            {
                let res = wave_container.query_variable(variable, &BigUint::ZERO).ok();
                res.and_then(|o| o.and_then(|q| q.current.map(|v| format!(": {}", v.1))))
                    .unwrap_or_else(|| ": Undefined".to_string())
            } else {
                String::new()
            };

            ui.with_layout(
                Layout::top_down(Align::LEFT).with_cross_justify(true),
                |ui| {
                    let mut label = LayoutJob::default();

                    match name_info.and_then(|info| info.true_name) {
                        Some(name) => {
                            // NOTE: Safe unwrap, we know that egui has its own built-in font
                            let font = ui.style().text_styles.get(&TextStyle::Monospace).unwrap();
                            let char_width = ui.fonts(|fonts| {
                                fonts
                                    .layout_no_wrap(
                                        " ".to_string(),
                                        font.clone(),
                                        Color32::from_rgb(0, 0, 0),
                                    )
                                    .size()
                                    .x
                            });

                            let direction_size = direction.chars().count();
                            let index_size = index.chars().count();
                            let value_size = value.chars().count();
                            let used_space =
                                (direction_size + index_size + value_size) as f32 * char_width;
                            // The button padding is added by egui on selectable labels
                            let available_space =
                                ui.available_width() - ui.spacing().button_padding.x * 2.;
                            let space_for_name = available_space - used_space;

                            let text_format = TextFormat {
                                font_id: font.clone(),
                                color: self.user.config.theme.foreground,
                                ..Default::default()
                            };

                            label.append(&direction, 0.0, text_format.clone());

                            draw_true_name(
                                &name,
                                &mut label,
                                font.clone(),
                                self.user.config.theme.foreground,
                                char_width,
                                space_for_name,
                            );

                            label.append(&index, 0.0, text_format.clone());
                            label.append(&value, 0.0, text_format.clone());
                        }
                        None => {
                            let font = ui.style().text_styles.get(&TextStyle::Body).unwrap();
                            let text_format = TextFormat {
                                font_id: font.clone(),
                                color: self.user.config.theme.foreground,
                                ..Default::default()
                            };
                            label.append(&direction, 0.0, text_format.clone());
                            label.append(&variable.name, 0.0, text_format.clone());
                            label.append(&index, 0.0, text_format.clone());
                            label.append(&value, 0.0, text_format.clone());
                        }
                    }

                    let mut response = ui.add(egui::SelectableLabel::new(false, label));

                    let _ = response.interact(egui::Sense::click_and_drag());

                    if self.show_tooltip() {
                        // Should be possible to reuse the meta from above?
                        response = response.on_hover_ui(|ui| {
                            let meta = wave_container.variable_meta(variable).ok();
                            ui.set_max_width(ui.spacing().tooltip_width);
                            ui.add(egui::Label::new(variable_tooltip_text(&meta, variable)));
                        });
                    }
                    response.drag_started().then(|| {
                        msgs.push(Message::VariableDragStarted(VisibleItemIndex(
                            self.user.waves.as_ref().unwrap().display_item_ref_counter,
                        )))
                    });
                    response.drag_stopped().then(|| {
                        if ui.input(|i| i.pointer.hover_pos().unwrap_or_default().x)
                            > self.user.sidepanel_width.unwrap_or_default()
                        {
                            msgs.push(Message::AddDraggedVariables(vec![variable.clone()]));
                        }
                    });
                    response
                        .clicked()
                        .then(|| msgs.push(Message::AddVariables(vec![variable.clone()])));
                },
            );
        }
    }

    fn draw_item_focus_list(&self, ui: &mut egui::Ui) {
        let alignment = self.get_name_alignment();
        ui.with_layout(
            Layout::top_down(alignment).with_cross_justify(false),
            |ui| {
                if self.show_default_timeline() {
                    ui.add_space(ui.text_style_height(&egui::TextStyle::Body) + 2.0);
                }
                for (vidx, _) in self
                    .user
                    .waves
                    .as_ref()
                    .unwrap()
                    .items_tree
                    .iter_visible()
                    .enumerate()
                {
                    let vidx = VisibleItemIndex(vidx);
                    ui.scope(|ui| {
                        ui.style_mut().visuals.selection.bg_fill =
                            self.user.config.theme.accent_warn.background;
                        ui.style_mut().visuals.override_text_color =
                            Some(self.user.config.theme.accent_warn.foreground);
                        let _ = ui.selectable_label(true, self.get_alpha_focus_id(vidx));
                    });
                }
            },
        );
    }

    fn hierarchy_icon(
        &self,
        ui: &mut egui::Ui,
        has_children: bool,
        unfolded: bool,
        alignment: Align,
    ) -> egui::Response {
        let (rect, response) = ui.allocate_exact_size(
            Vec2::splat(self.user.config.layout.waveforms_text_size),
            Sense::click(),
        );
        if !has_children {
            return response;
        }

        // fixme: use the much nicer remixicon arrow? do a layout here and paint the galley into the rect?
        // or alternatively: change how the tree iterator works and use the egui facilities (cross widget?)
        let icon_rect = Rect::from_center_size(
            rect.center(),
            emath::vec2(rect.width(), rect.height()) * 0.75,
        );
        let mut points = vec![
            icon_rect.left_top(),
            icon_rect.right_top(),
            icon_rect.center_bottom(),
        ];
        let rotation = emath::Rot2::from_angle(if unfolded {
            0.0
        } else if alignment == Align::LEFT {
            -std::f32::consts::TAU / 4.0
        } else {
            std::f32::consts::TAU / 4.0
        });
        for p in &mut points {
            *p = icon_rect.center() + rotation * (*p - icon_rect.center());
        }

        let style = ui.style().interact(&response);
        ui.painter().add(egui::Shape::convex_polygon(
            points,
            style.fg_stroke.color,
            egui::Stroke::NONE,
        ));
        response
    }

    fn draw_item_list(&mut self, msgs: &mut Vec<Message>, ui: &mut egui::Ui, ctx: &egui::Context) {
        let mut item_offsets = Vec::new();

        let any_groups = self
            .user
            .waves
            .as_ref()
            .unwrap()
            .items_tree
            .iter()
            .any(|node| node.level > 0);
        let alignment = self.get_name_alignment();
        ui.with_layout(Layout::top_down(alignment).with_cross_justify(true), |ui| {
            let available_rect = ui.available_rect_before_wrap();
            for crate::displayed_item_tree::Info {
                node:
                    crate::displayed_item_tree::Node {
                        item_ref,
                        level,
                        unfolded,
                        ..
                    },
                vidx,
                has_children,
                last,
                ..
            } in self
                .user
                .waves
                .as_ref()
                .unwrap()
                .items_tree
                .iter_visible_extra()
            {
                let Some(displayed_item) = self
                    .user
                    .waves
                    .as_ref()
                    .unwrap()
                    .displayed_items
                    .get(item_ref)
                else {
                    continue;
                };

                ui.with_layout(
                    if alignment == Align::LEFT {
                        Layout::left_to_right(Align::TOP)
                    } else {
                        Layout::right_to_left(Align::TOP)
                    },
                    |ui| {
                        ui.add_space(10.0 * *level as f32);
                        if any_groups {
                            let response =
                                self.hierarchy_icon(ui, has_children, *unfolded, alignment);
                            if response.clicked() {
                                if *unfolded {
                                    msgs.push(Message::GroupFold(Some(*item_ref)));
                                } else {
                                    msgs.push(Message::GroupUnfold(Some(*item_ref)));
                                }
                            }
                        }

                        let item_rect = match displayed_item {
                            DisplayedItem::Variable(displayed_variable) => {
                                let levels_to_force_expand =
                                    self.items_to_expand.borrow().iter().find_map(
                                        |(id, levels)| {
                                            if item_ref == id {
                                                Some(*levels)
                                            } else {
                                                None
                                            }
                                        },
                                    );

                                self.draw_variable(
                                    msgs,
                                    vidx,
                                    displayed_item,
                                    *item_ref,
                                    FieldRef::without_fields(
                                        displayed_variable.variable_ref.clone(),
                                    ),
                                    &mut item_offsets,
                                    &displayed_variable.info,
                                    ui,
                                    ctx,
                                    levels_to_force_expand,
                                    alignment,
                                )
                            }
                            DisplayedItem::Divider(_)
                            | DisplayedItem::Marker(_)
                            | DisplayedItem::Placeholder(_)
                            | DisplayedItem::TimeLine(_)
                            | DisplayedItem::Stream(_)
                            | DisplayedItem::Group(_) => {
                                ui.with_layout(
                                    ui.layout()
                                        .with_main_justify(true)
                                        .with_main_align(alignment),
                                    |ui| {
                                        self.draw_plain_item(
                                            msgs,
                                            vidx,
                                            *item_ref,
                                            displayed_item,
                                            &mut item_offsets,
                                            ui,
                                            ctx,
                                        )
                                    },
                                )
                                .inner
                            }
                        };
                        // expand to the left, but not over the icon size
                        let mut expanded_rect = item_rect;
                        expanded_rect.set_left(
                            available_rect.left()
                                + self.user.config.layout.waveforms_text_size
                                + ui.spacing().item_spacing.x,
                        );
                        expanded_rect.set_right(available_rect.right());
                        self.draw_drag_target(msgs, vidx, expanded_rect, available_rect, ui, last);
                    },
                );
            }
        });

        self.user.waves.as_mut().unwrap().drawing_infos = item_offsets;
    }

    fn draw_transaction_root(
        &self,
        msgs: &mut Vec<Message>,
        streams: &WaveData,
        ui: &mut egui::Ui,
    ) {
        egui::collapsing_header::CollapsingState::load_with_default_open(
            ui.ctx(),
            egui::Id::from("Streams"),
            false,
        )
        .show_header(ui, |ui| {
            ui.with_layout(
                Layout::top_down(Align::LEFT).with_cross_justify(true),
                |ui| {
                    let root_name = String::from("tr");
                    let response = ui.add(egui::SelectableLabel::new(
                        streams.active_scope == Some(ScopeType::StreamScope(StreamScopeRef::Root)),
                        root_name,
                    ));

                    response.clicked().then(|| {
                        msgs.push(Message::SetActiveScope(ScopeType::StreamScope(
                            StreamScopeRef::Root,
                        )));
                    });
                },
            );
        })
        .body(|ui| {
            for (id, stream) in &streams.inner.as_transactions().unwrap().inner.tx_streams {
                let name = stream.name.clone();
                let response = ui.add(egui::SelectableLabel::new(
                    streams.active_scope.as_ref().is_some_and(|s| {
                        if let ScopeType::StreamScope(StreamScopeRef::Stream(scope_stream)) = s {
                            scope_stream.stream_id == *id
                        } else {
                            false
                        }
                    }),
                    name.clone(),
                ));

                response.clicked().then(|| {
                    msgs.push(Message::SetActiveScope(ScopeType::StreamScope(
                        StreamScopeRef::Stream(TransactionStreamRef::new_stream(*id, name)),
                    )));
                });
            }
        });
    }

    pub fn draw_transaction_variable_list(
        &self,
        msgs: &mut Vec<Message>,
        streams: &WaveData,
        ui: &mut egui::Ui,
        active_stream: &StreamScopeRef,
    ) {
        let inner = streams.inner.as_transactions().unwrap();
        match active_stream {
            StreamScopeRef::Root => {
                for stream in inner.get_streams() {
                    ui.with_layout(
                        Layout::top_down(Align::LEFT).with_cross_justify(true),
                        |ui| {
                            let response =
                                ui.add(egui::SelectableLabel::new(false, stream.name.clone()));

                            response.clicked().then(|| {
                                msgs.push(Message::AddStreamOrGenerator(
                                    TransactionStreamRef::new_stream(
                                        stream.id,
                                        stream.name.clone(),
                                    ),
                                ));
                            });
                        },
                    );
                }
            }
            StreamScopeRef::Stream(stream_ref) => {
                for gen_id in &inner.get_stream(stream_ref.stream_id).unwrap().generators {
                    let gen_name = inner.get_generator(*gen_id).unwrap().name.clone();
                    ui.with_layout(
                        Layout::top_down(Align::LEFT).with_cross_justify(true),
                        |ui| {
                            let response = ui.add(egui::SelectableLabel::new(false, &gen_name));

                            response.clicked().then(|| {
                                msgs.push(Message::AddStreamOrGenerator(
                                    TransactionStreamRef::new_gen(
                                        stream_ref.stream_id,
                                        *gen_id,
                                        gen_name,
                                    ),
                                ));
                            });
                        },
                    );
                }
            }
            StreamScopeRef::Empty(_) => {}
        }
    }
    fn draw_focused_transaction_details(&self, ui: &mut egui::Ui) {
        ui.with_layout(
            Layout::top_down(Align::LEFT).with_cross_justify(true),
            |ui| {
                ui.label("Focused Transaction Details");
                let column_width = ui.available_width() / 2.;
                TableBuilder::new(ui)
                    .column(Column::exact(column_width))
                    .column(Column::auto())
                    .header(20.0, |mut header| {
                        header.col(|ui| {
                            ui.heading("Properties");
                        });
                    })
                    .body(|mut body| {
                        let focused_transaction = self
                            .user
                            .waves
                            .as_ref()
                            .unwrap()
                            .focused_transaction
                            .1
                            .as_ref()
                            .unwrap();
                        let row_height = 15.;
                        body.row(row_height, |mut row| {
                            row.col(|ui| {
                                ui.label("Transaction ID");
                            });
                            row.col(|ui| {
                                ui.label(focused_transaction.get_tx_id().to_string());
                            });
                        });
                        body.row(row_height, |mut row| {
                            row.col(|ui| {
                                ui.label("Type");
                            });
                            row.col(|ui| {
                                let gen = self
                                    .user
                                    .waves
                                    .as_ref()
                                    .unwrap()
                                    .inner
                                    .as_transactions()
                                    .unwrap()
                                    .get_generator(focused_transaction.get_gen_id())
                                    .unwrap();
                                ui.label(gen.name.to_string());
                            });
                        });
                        body.row(row_height, |mut row| {
                            row.col(|ui| {
                                ui.label("Start Time");
                            });
                            row.col(|ui| {
                                ui.label(focused_transaction.get_start_time().to_string());
                            });
                        });
                        body.row(row_height, |mut row| {
                            row.col(|ui| {
                                ui.label("End Time");
                            });
                            row.col(|ui| {
                                ui.label(focused_transaction.get_end_time().to_string());
                            });
                        });
                        body.row(row_height + 5., |mut row| {
                            row.col(|ui| {
                                ui.heading("Attributes");
                            });
                        });

                        body.row(row_height + 3., |mut row| {
                            row.col(|ui| {
                                ui.label(RichText::new("Name").size(15.));
                            });
                            row.col(|ui| {
                                ui.label(RichText::new("Value").size(15.));
                            });
                        });

                        for attr in &focused_transaction.attributes {
                            body.row(row_height, |mut row| {
                                row.col(|ui| {
                                    ui.label(attr.name.to_string());
                                });
                                row.col(|ui| {
                                    ui.label(attr.value().to_string());
                                });
                            });
                        }

                        if !focused_transaction.inc_relations.is_empty() {
                            body.row(row_height + 5., |mut row| {
                                row.col(|ui| {
                                    ui.heading("Incoming Relations");
                                });
                            });

                            body.row(row_height + 3., |mut row| {
                                row.col(|ui| {
                                    ui.label(RichText::new("Source Tx").size(15.));
                                });
                                row.col(|ui| {
                                    ui.label(RichText::new("Sink Tx").size(15.));
                                });
                            });

                            for rel in &focused_transaction.inc_relations {
                                body.row(row_height, |mut row| {
                                    row.col(|ui| {
                                        ui.label(rel.source_tx_id.to_string());
                                    });
                                    row.col(|ui| {
                                        ui.label(rel.sink_tx_id.to_string());
                                    });
                                });
                            }
                        }

                        if !focused_transaction.out_relations.is_empty() {
                            body.row(row_height + 5., |mut row| {
                                row.col(|ui| {
                                    ui.heading("Outgoing Relations");
                                });
                            });

                            body.row(row_height + 3., |mut row| {
                                row.col(|ui| {
                                    ui.label(RichText::new("Source Tx").size(15.));
                                });
                                row.col(|ui| {
                                    ui.label(RichText::new("Sink Tx").size(15.));
                                });
                            });

                            for rel in &focused_transaction.out_relations {
                                body.row(row_height, |mut row| {
                                    row.col(|ui| {
                                        ui.label(rel.source_tx_id.to_string());
                                    });
                                    row.col(|ui| {
                                        ui.label(rel.sink_tx_id.to_string());
                                    });
                                });
                            }
                        }
                    });
            },
        );
    }

    fn get_name_alignment(&self) -> Align {
        if self
            .user
            .align_names_right
            .unwrap_or_else(|| self.user.config.layout.align_names_right())
        {
            Align::RIGHT
        } else {
            Align::LEFT
        }
    }

    fn draw_drag_source(
        &self,
        msgs: &mut Vec<Message>,
        vidx: VisibleItemIndex,
        item_response: &egui::Response,
        modifiers: egui::Modifiers,
    ) {
        if item_response.dragged_by(egui::PointerButton::Primary)
            && item_response.drag_delta().length() > self.user.config.theme.drag_threshold
        {
            if !modifiers.ctrl
                && !(self.user.waves.as_ref())
                    .and_then(|w| w.items_tree.get_visible(vidx))
                    .map(|i| i.selected)
                    .unwrap_or(false)
            {
                msgs.push(Message::FocusItem(vidx));
                msgs.push(Message::ItemSelectionClear);
            }
            msgs.push(Message::SetItemSelected(vidx, true));
            msgs.push(Message::VariableDragStarted(vidx));
        }

        if item_response.drag_stopped()
            && self
                .user
                .drag_source_idx
                .is_some_and(|source_idx| source_idx == vidx)
        {
            msgs.push(Message::VariableDragFinished);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_variable_label(
        &self,
        vidx: VisibleItemIndex,
        displayed_item: &DisplayedItem,
        displayed_id: DisplayedItemRef,
        field: FieldRef,
        msgs: &mut Vec<Message>,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
    ) -> egui::Response {
        let mut variable_label = self.draw_item_label(
            vidx,
            displayed_id,
            displayed_item,
            Some(&field),
            msgs,
            ui,
            ctx,
        );

        if self.show_tooltip() {
            variable_label = variable_label.on_hover_ui(|ui| {
                let tooltip = if let Some(waves) = &self.user.waves {
                    if field.field.is_empty() {
                        let wave_container = waves.inner.as_waves().unwrap();
                        let meta = wave_container.variable_meta(&field.root).ok();
                        variable_tooltip_text(&meta, &field.root)
                    } else {
                        "From translator".to_string()
                    }
                } else {
                    "No VCD loaded".to_string()
                };
                ui.set_max_width(ui.spacing().tooltip_width);
                ui.add(egui::Label::new(tooltip));
            });
        }

        variable_label
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_variable(
        &self,
        msgs: &mut Vec<Message>,
        vidx: VisibleItemIndex,
        displayed_item: &DisplayedItem,
        displayed_id: DisplayedItemRef,
        field: FieldRef,
        drawing_infos: &mut Vec<ItemDrawingInfo>,
        info: &VariableInfo,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        levels_to_force_expand: Option<usize>,
        alignment: Align,
    ) -> Rect {
        let displayed_field_ref = DisplayedFieldRef {
            item: displayed_id,
            field: field.field.clone(),
        };
        match info {
            VariableInfo::Compound { subfields } => {
                let mut header = egui::collapsing_header::CollapsingState::load_with_default_open(
                    ui.ctx(),
                    egui::Id::new(&field),
                    false,
                );

                if let Some(level) = levels_to_force_expand {
                    header.set_open(level > 0);
                }

                let response = ui
                    .with_layout(Layout::top_down(alignment).with_cross_justify(true), |ui| {
                        header
                            .show_header(ui, |ui| {
                                ui.with_layout(
                                    Layout::top_down(alignment).with_cross_justify(true),
                                    |ui| {
                                        self.draw_variable_label(
                                            vidx,
                                            displayed_item,
                                            displayed_id,
                                            field.clone(),
                                            msgs,
                                            ui,
                                            ctx,
                                        )
                                    },
                                );
                            })
                            .body(|ui| {
                                for (name, info) in subfields {
                                    let mut new_path = field.clone();
                                    new_path.field.push(name.clone());
                                    ui.with_layout(
                                        Layout::top_down(alignment).with_cross_justify(true),
                                        |ui| {
                                            self.draw_variable(
                                                msgs,
                                                vidx,
                                                displayed_item,
                                                displayed_id,
                                                new_path,
                                                drawing_infos,
                                                info,
                                                ui,
                                                ctx,
                                                levels_to_force_expand.map(|l| l.saturating_sub(1)),
                                                alignment,
                                            );
                                        },
                                    )
                                    .inner
                                }
                            })
                    })
                    .inner;
                drawing_infos.push(ItemDrawingInfo::Variable(VariableDrawingInfo {
                    displayed_field_ref,
                    field_ref: field.clone(),
                    item_list_idx: vidx,
                    top: response.0.rect.top(),
                    bottom: response.0.rect.bottom(),
                }));
                response.0.rect
            }
            VariableInfo::Bool
            | VariableInfo::Bits
            | VariableInfo::Clock
            | VariableInfo::String
            | VariableInfo::Real => {
                let label = ui
                    .with_layout(Layout::top_down(alignment).with_cross_justify(true), |ui| {
                        self.draw_variable_label(
                            vidx,
                            displayed_item,
                            displayed_id,
                            field.clone(),
                            msgs,
                            ui,
                            ctx,
                        )
                    })
                    .inner;
                self.draw_drag_source(msgs, vidx, &label, ctx.input(|e| e.modifiers));
                drawing_infos.push(ItemDrawingInfo::Variable(VariableDrawingInfo {
                    displayed_field_ref,
                    field_ref: field.clone(),
                    item_list_idx: vidx,
                    top: label.rect.top(),
                    bottom: label.rect.bottom(),
                }));
                label.rect
            }
        }
    }

    fn draw_drag_target(
        &self,
        msgs: &mut Vec<Message>,
        vidx: VisibleItemIndex,
        expanded_rect: Rect,
        available_rect: Rect,
        ui: &mut egui::Ui,
        last: bool,
    ) {
        if !self.user.drag_started || self.user.drag_source_idx.is_none() {
            return;
        }

        let waves = self
            .user
            .waves
            .as_ref()
            .expect("waves not available, but expected");

        // expanded_rect is just for the label, leaving us with gaps between lines
        // expand to counter that
        let rect_with_margin = expanded_rect.expand2(ui.spacing().item_spacing / 2f32);

        // collision check rect need to be
        // - limited to half the height of the item text
        // - extended to cover the empty space to the left
        // - for the last element, expanded till the bottom
        let before_rect = rect_with_margin
            .with_max_y(rect_with_margin.left_center().y)
            .with_min_x(available_rect.left())
            .round_to_pixels(ui.painter().pixels_per_point());
        let after_rect = if last {
            rect_with_margin.with_max_y(ui.max_rect().max.y)
        } else {
            rect_with_margin
        }
        .with_min_y(rect_with_margin.left_center().y)
        .with_min_x(available_rect.left())
        .round_to_pixels(ui.painter().pixels_per_point());

        let (insert_vidx, line_y) = if ui.rect_contains_pointer(before_rect) {
            (vidx, rect_with_margin.top())
        } else if ui.rect_contains_pointer(after_rect) {
            (VisibleItemIndex(vidx.0 + 1), rect_with_margin.bottom())
        } else {
            return;
        };

        let level_range = waves.items_tree.valid_levels_visible(insert_vidx, |node| {
            matches!(
                waves.displayed_items.get(&node.item_ref),
                Some(DisplayedItem::Group(..))
            )
        });

        let left_x = |level: u8| -> f32 { rect_with_margin.left() + level as f32 * 10.0 };
        let Some(insert_level) = level_range.find_or_last(|&level| {
            let mut rect = expanded_rect.with_min_x(left_x(level));
            rect.set_width(10.0);
            if level == 0 {
                rect.set_left(available_rect.left());
            }
            ui.rect_contains_pointer(rect)
        }) else {
            return;
        };

        ui.painter().line_segment(
            [
                Pos2::new(left_x(insert_level), line_y),
                Pos2::new(rect_with_margin.right(), line_y),
            ],
            Stroke::new(
                self.user.config.theme.linewidth,
                self.user.config.theme.drag_hint_color,
            ),
        );
        msgs.push(Message::VariableDragTargetChanged(
            crate::displayed_item_tree::TargetPosition {
                before: ItemIndex(
                    waves
                        .items_tree
                        .to_displayed(insert_vidx)
                        .map(|index| index.0)
                        .unwrap_or_else(|| waves.items_tree.len()),
                ),
                level: insert_level,
            },
        ));
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_item_label(
        &self,
        vidx: VisibleItemIndex,
        displayed_id: DisplayedItemRef,
        displayed_item: &DisplayedItem,
        field: Option<&FieldRef>,
        msgs: &mut Vec<Message>,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
    ) -> egui::Response {
        let text_color = {
            let style = ui.style_mut();
            if self.item_is_focused(vidx) {
                style.visuals.selection.bg_fill = self.user.config.theme.accent_info.background;
                self.user.config.theme.accent_info.foreground
            } else if self.item_is_selected(displayed_id) {
                style.visuals.selection.bg_fill =
                    self.user.config.theme.selected_elements_colors.background;
                self.user.config.theme.selected_elements_colors.foreground
            } else if matches!(
                displayed_item,
                DisplayedItem::Variable(_) | DisplayedItem::Placeholder(_)
            ) {
                style.visuals.selection.bg_fill =
                    self.user.config.theme.primary_ui_color.background;
                self.user.config.theme.primary_ui_color.foreground
            } else {
                style.visuals.selection.bg_fill =
                    self.user.config.theme.primary_ui_color.background;
                *self.get_item_text_color(displayed_item)
            }
        };

        let monospace_font = ui.style().text_styles.get(&TextStyle::Monospace).unwrap();
        let monospace_width = {
            ui.fonts(|fonts| {
                fonts
                    .layout_no_wrap(" ".to_string(), monospace_font.clone(), Color32::BLACK)
                    .size()
                    .x
            })
        };
        let available_space = ui.available_width();

        let mut layout_job = LayoutJob::default();
        match displayed_item {
            DisplayedItem::Variable(var) if field.is_some() => {
                let field = field.unwrap();
                if field.field.is_empty() {
                    let wave_container =
                        self.user.waves.as_ref().unwrap().inner.as_waves().unwrap();
                    let name_info = self.get_variable_name_info(wave_container, &var.variable_ref);

                    if let Some(true_name) = name_info.and_then(|info| info.true_name) {
                        draw_true_name(
                            &true_name,
                            &mut layout_job,
                            monospace_font.clone(),
                            text_color,
                            monospace_width,
                            available_space,
                        )
                    } else {
                        displayed_item.add_to_layout_job(
                            &text_color,
                            ui.style(),
                            &mut layout_job,
                            Some(field),
                            &self.user.config,
                        )
                    }
                } else {
                    RichText::new(field.field.last().unwrap().clone())
                        .color(text_color)
                        .line_height(Some(self.user.config.layout.waveforms_line_height))
                        .append_to(
                            &mut layout_job,
                            ui.style(),
                            FontSelection::Default,
                            Align::Center,
                        )
                }
            }
            _ => displayed_item.add_to_layout_job(
                &text_color,
                ui.style(),
                &mut layout_job,
                field,
                &self.user.config,
            ),
        }

        let item_label = ui
            .selectable_label(
                self.item_is_selected(displayed_id) || self.item_is_focused(vidx),
                WidgetText::LayoutJob(layout_job),
            )
            .interact(Sense::drag());
        item_label.context_menu(|ui| {
            self.item_context_menu(field, msgs, ui, vidx);
        });

        if item_label.clicked() {
            let focused = self.user.waves.as_ref().and_then(|w| w.focused_item);
            let was_focused = focused == Some(vidx);
            if was_focused {
                msgs.push(Message::UnfocusItem);
            } else {
                let modifiers = ctx.input(|i| i.modifiers);
                if modifiers.ctrl {
                    msgs.push(Message::ToggleItemSelected(Some(vidx)));
                } else if modifiers.shift {
                    msgs.push(Message::Batch(vec![
                        Message::ItemSelectionClear,
                        Message::ItemSelectRange(vidx),
                    ]));
                } else {
                    msgs.push(Message::Batch(vec![
                        Message::ItemSelectionClear,
                        Message::FocusItem(vidx),
                    ]));
                }
            }
        }

        item_label
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_plain_item(
        &self,
        msgs: &mut Vec<Message>,
        vidx: VisibleItemIndex,
        displayed_id: DisplayedItemRef,
        displayed_item: &DisplayedItem,
        drawing_infos: &mut Vec<ItemDrawingInfo>,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
    ) -> Rect {
        let label = self.draw_item_label(vidx, displayed_id, displayed_item, None, msgs, ui, ctx);

        self.draw_drag_source(msgs, vidx, &label, ui.ctx().input(|e| e.modifiers));
        match displayed_item {
            DisplayedItem::Divider(_) => {
                drawing_infos.push(ItemDrawingInfo::Divider(DividerDrawingInfo {
                    item_list_idx: vidx,
                    top: label.rect.top(),
                    bottom: label.rect.bottom(),
                }));
            }
            DisplayedItem::Marker(cursor) => {
                drawing_infos.push(ItemDrawingInfo::Marker(MarkerDrawingInfo {
                    item_list_idx: vidx,
                    top: label.rect.top(),
                    bottom: label.rect.bottom(),
                    idx: cursor.idx,
                }));
            }
            DisplayedItem::TimeLine(_) => {
                drawing_infos.push(ItemDrawingInfo::TimeLine(TimeLineDrawingInfo {
                    item_list_idx: vidx,
                    top: label.rect.top(),
                    bottom: label.rect.bottom(),
                }));
            }
            DisplayedItem::Stream(stream) => {
                drawing_infos.push(ItemDrawingInfo::Stream(StreamDrawingInfo {
                    transaction_stream_ref: stream.transaction_stream_ref.clone(),
                    item_list_idx: vidx,
                    top: label.rect.top(),
                    bottom: label.rect.bottom(),
                }));
            }
            DisplayedItem::Group(_) => {
                drawing_infos.push(ItemDrawingInfo::Group(GroupDrawingInfo {
                    item_list_idx: vidx,
                    top: label.rect.top(),
                    bottom: label.rect.bottom(),
                }));
            }
            &DisplayedItem::Variable(_) => {}
            &DisplayedItem::Placeholder(_) => {}
        }
        label.rect
    }

    fn get_alpha_focus_id(&self, vidx: VisibleItemIndex) -> RichText {
        let alpha_id = uint_idx_to_alpha_idx(
            vidx,
            self.user
                .waves
                .as_ref()
                .map_or(0, |waves| waves.displayed_items.len()),
        );

        RichText::new(alpha_id).monospace()
    }

    fn item_is_focused(&self, vidx: VisibleItemIndex) -> bool {
        if let Some(waves) = &self.user.waves {
            waves.focused_item == Some(vidx)
        } else {
            false
        }
    }

    fn item_is_selected(&self, id: DisplayedItemRef) -> bool {
        if let Some(waves) = &self.user.waves {
            waves
                .items_tree
                .iter_visible_selected()
                .any(|node| node.item_ref == id)
        } else {
            false
        }
    }

    fn draw_var_values(&self, ui: &mut egui::Ui, msgs: &mut Vec<Message>) {
        let Some(waves) = &self.user.waves else {
            return;
        };
        let (response, mut painter) = ui.allocate_painter(ui.available_size(), Sense::click());
        let rect = response.rect;
        let container_rect = Rect::from_min_size(Pos2::ZERO, rect.size());
        let to_screen = RectTransform::from_to(container_rect, rect);
        let cfg = DrawConfig::new(
            rect.height(),
            self.user.config.layout.waveforms_line_height,
            self.user.config.layout.waveforms_text_size,
        );
        let frame_width = rect.width();

        painter.rect_filled(
            rect,
            CornerRadiusF32::ZERO,
            self.user.config.theme.secondary_ui_color.background,
        );
        let ctx = DrawingContext {
            painter: &mut painter,
            cfg: &cfg,
            // This 0.5 is very odd, but it fixes the lines we draw being smushed out across two
            // pixels, resulting in dimmer colors https://github.com/emilk/egui/issues/1322
            to_screen: &|x, y| to_screen.transform_pos(Pos2::new(x, y) + Vec2::new(0.5, 0.5)),
            theme: &self.user.config.theme,
        };

        let gap = ui.spacing().item_spacing.y * 0.5;
        let y_zero = to_screen.transform_pos(Pos2::ZERO).y;
        let ucursor = waves.cursor.as_ref().and_then(num::BigInt::to_biguint);

        // Add default margin as it was removed when creating the frame
        let rect_with_margin = Rect {
            min: rect.min + ui.spacing().item_spacing,
            max: rect.max,
        };

        let builder = UiBuilder::new().max_rect(rect_with_margin);
        ui.allocate_new_ui(builder, |ui| {
            let text_style = TextStyle::Monospace;
            ui.style_mut().override_text_style = Some(text_style);
            for (vidx, drawing_info) in waves
                .drawing_infos
                .iter()
                .sorted_by_key(|o| o.top() as i32)
                .enumerate()
            {
                let vidx = VisibleItemIndex(vidx);
                let next_y = ui.cursor().top();
                // In order to align the text in this view with the variable tree,
                // we need to keep track of how far away from the expected offset we are,
                // and compensate for it
                if next_y < drawing_info.top() {
                    ui.add_space(drawing_info.top() - next_y);
                }

                let backgroundcolor = &self.get_background_color(waves, drawing_info, vidx);
                self.draw_background(
                    drawing_info,
                    y_zero,
                    &ctx,
                    gap,
                    frame_width,
                    backgroundcolor,
                );
                match drawing_info {
                    ItemDrawingInfo::Variable(drawing_info) => {
                        if ucursor.as_ref().is_none() {
                            ui.label("");
                            continue;
                        }

                        let v = self.get_variable_value(
                            waves,
                            &drawing_info.displayed_field_ref,
                            &ucursor,
                        );
                        if let Some(v) = v {
                            ui.label(RichText::new(v).color(
                                *self.user.config.theme.get_best_text_color(backgroundcolor),
                            ))
                            .context_menu(|ui| {
                                self.item_context_menu(
                                    Some(&FieldRef::without_fields(
                                        drawing_info.field_ref.root.clone(),
                                    )),
                                    msgs,
                                    ui,
                                    vidx,
                                );
                            });
                        }
                    }

                    ItemDrawingInfo::Marker(numbered_cursor) => {
                        if let Some(cursor) = &waves.cursor {
                            let delta = time_string(
                                &(waves.numbered_marker_time(numbered_cursor.idx) - cursor),
                                &waves.inner.metadata().timescale,
                                &self.user.wanted_timeunit,
                                &self.get_time_format(),
                            );

                            ui.label(RichText::new(format!("Δ: {delta}",)).color(
                                *self.user.config.theme.get_best_text_color(backgroundcolor),
                            ))
                            .context_menu(|ui| {
                                self.item_context_menu(None, msgs, ui, vidx);
                            });
                        } else {
                            ui.label("");
                        }
                    }
                    ItemDrawingInfo::Divider(_)
                    | ItemDrawingInfo::TimeLine(_)
                    | ItemDrawingInfo::Stream(_)
                    | ItemDrawingInfo::Group(_) => {
                        ui.label("");
                    }
                }
            }
        });
    }

    pub fn get_variable_value(
        &self,
        waves: &WaveData,
        displayed_field_ref: &DisplayedFieldRef,
        ucursor: &Option<num::BigUint>,
    ) -> Option<String> {
        if let Some(ucursor) = ucursor {
            let Some(DisplayedItem::Variable(displayed_variable)) =
                waves.displayed_items.get(&displayed_field_ref.item)
            else {
                return None;
            };
            let variable = &displayed_variable.variable_ref;
            let translator =
                waves.variable_translator(&displayed_field_ref.without_field(), &self.translators);
            let meta = waves.inner.as_waves().unwrap().variable_meta(variable);

            let translation_result = waves
                .inner
                .as_waves()
                .unwrap()
                .query_variable(variable, ucursor)
                .ok()
                .flatten()
                .and_then(|q| q.current)
                .map(|(_time, value)| meta.and_then(|meta| translator.translate(&meta, &value)));

            if let Some(Ok(s)) = translation_result {
                let fields = s.format_flat(
                    &displayed_variable.format,
                    &displayed_variable.field_formats,
                    &self.translators,
                );

                let subfield = fields
                    .iter()
                    .find(|res| res.names == displayed_field_ref.field);

                if let Some(SubFieldFlatTranslationResult {
                    names: _,
                    value: Some(TranslatedValue { value: v, kind: _ }),
                }) = subfield
                {
                    Some(v.clone())
                } else {
                    Some("-".to_string())
                }
            } else {
                None
            }
        } else {
            None
        }
    }

pub fn get_variable_name_info(
    &self,
    wave_container: &WaveContainer,
    var: &VariableRef,
) -> Option<VariableNameInfo> {
    // First, attempt a safe, read-only check of the cache.
    if let Some(info) = self.variable_name_info_cache.borrow().get(var) {
        return info.clone(); // If the name is already cached, return it.
    }

    // If not in the cache, try for a mutable lock to insert the new name.
    if let Ok(mut cache) = self.variable_name_info_cache.try_borrow_mut() {
        let meta = wave_container.variable_meta(var).ok();

        let entry = cache.entry(var.clone()).or_insert_with(|| {
            meta.as_ref().and_then(|meta| {
                self.translators
                    .all_translators()
                    .iter()
                    .find_map(|t| t.variable_name_info(meta))
            })
        });

        // FIX: Return the cloned entry directly. It is already an Option<VariableNameInfo>.
        entry.clone()
    } else {
        // If getting a mutable lock fails (due to a nested call),
        // gracefully return `None` instead of crashing.
        None
    }
}

    pub fn draw_background(
        &self,
        drawing_info: &ItemDrawingInfo,
        y_zero: f32,
        ctx: &DrawingContext<'_>,
        gap: f32,
        frame_width: f32,
        background_color: &Color32,
    ) {
        // Draw background
        let min = (ctx.to_screen)(0.0, drawing_info.top() - y_zero - gap);
        let max = (ctx.to_screen)(frame_width, drawing_info.bottom() - y_zero + gap);
        ctx.painter
            .rect_filled(Rect { min, max }, CornerRadiusF32::ZERO, *background_color);
    }

    pub fn get_background_color(
        &self,
        waves: &WaveData,
        drawing_info: &ItemDrawingInfo,
        vidx: VisibleItemIndex,
    ) -> Color32 {
        if let Some(focused) = waves.focused_item {
            if self.highlight_focused() && focused == vidx {
                return self.user.config.theme.highlight_background;
            }
        }
        *waves
            .displayed_items
            .get(
                &waves
                    .items_tree
                    .get_visible(drawing_info.item_list_idx())
                    .unwrap()
                    .item_ref,
            )
            .and_then(super::displayed_item::DisplayedItem::background_color)
            .and_then(|color| self.user.config.theme.get_color(color))
            .unwrap_or_else(|| self.get_default_alternating_background_color(vidx))
    }

    fn get_default_alternating_background_color(&self, vidx: VisibleItemIndex) -> &Color32 {
        // Set background color
        if self.user.config.theme.alt_frequency != 0
            && (vidx.0 / self.user.config.theme.alt_frequency) % 2 == 1
        {
            &self.user.config.theme.canvas_colors.alt_background
        } else {
            &Color32::TRANSPARENT
        }
    }

    /// Draw the default timeline at the top of the canvas
    pub fn draw_default_timeline(
        &self,
        waves: &WaveData,
        ctx: &DrawingContext,
        viewport_idx: usize,
        frame_width: f32,
        cfg: &DrawConfig,
    ) {
        let ticks = waves.get_ticks(
            &waves.viewports[viewport_idx],
            &waves.inner.metadata().timescale,
            frame_width,
            cfg.text_size,
            &self.user.wanted_timeunit,
            &self.get_time_format(),
            &self.user.config,
        );

        waves.draw_ticks(
            Some(&self.user.config.theme.foreground),
            &ticks,
            ctx,
            0.0,
            egui::Align2::CENTER_TOP,
            &self.user.config,
        );
    }
}

fn variable_tooltip_text(meta: &Option<VariableMeta>, variable: &VariableRef) -> String {
    if let Some(meta) = meta {
        format!(
            "{}\nNum bits: {}\nType: {}\nDirection: {}",
            variable.full_path_string(),
            meta.num_bits
                .map_or_else(|| "unknown".to_string(), |bits| bits.to_string()),
            meta.variable_type_name
                .clone()
                .or_else(|| meta.variable_type.map(|t| t.to_string()))
                .unwrap_or_else(|| "unknown".to_string()),
            meta.direction
                .map_or_else(|| "unknown".to_string(), |direction| format!("{direction}"))
        )
    } else {
        variable.full_path_string()
    }
}

fn scope_tooltip_text(wave: &WaveData, scope: &ScopeRef) -> String {
    let other = wave.inner.as_waves().unwrap().get_scope_tooltip_data(scope);
    if other.is_empty() {
        format!("{scope}")
    } else {
        format!("{scope}\n{other}")
    }
}

pub fn draw_true_name(
    true_name: &TrueName,
    layout_job: &mut LayoutJob,
    font: FontId,
    foreground: Color32,
    char_width: f32,
    allowed_space: f32,
) {
    let char_budget = (allowed_space / char_width) as usize;

    match true_name {
        TrueName::SourceCode {
            line_number,
            before,
            this,
            after,
        } => {
            let before_chars = before.chars().collect::<Vec<_>>();
            let this_chars = this.chars().collect::<Vec<_>>();
            let after_chars = after.chars().collect::<Vec<_>>();
            let line_num = format!("{line_number} ");
            let important_chars = line_num.len() + this_chars.len();
            let required_extra_chars = before_chars.len() + after_chars.len();

            // If everything fits, things are very easy
            let (line_num, before, this, after) =
                if char_budget >= important_chars + required_extra_chars {
                    (line_num, before.clone(), this.clone(), after.clone())
                } else if char_budget > important_chars {
                    // How many extra chars we have available
                    let extra_chars = char_budget - important_chars;

                    let max_from_before = (extra_chars as f32 / 2.).ceil() as usize;
                    let max_from_after = (extra_chars as f32 / 2.).floor() as usize;

                    let (chars_from_before, chars_from_after) =
                        if max_from_before > before_chars.len() {
                            (before_chars.len(), extra_chars - before_chars.len())
                        } else if max_from_after > after_chars.len() {
                            (extra_chars - after_chars.len(), before_chars.len())
                        } else {
                            (max_from_before, max_from_after)
                        };

                    let mut before = before_chars
                        .into_iter()
                        .rev()
                        .take(chars_from_before)
                        .rev()
                        .collect::<Vec<_>>();
                    if !before.is_empty() {
                        before[0] = '…'
                    }
                    let mut after = after_chars
                        .into_iter()
                        .take(chars_from_after)
                        .collect::<Vec<_>>();
                    if !after.is_empty() {
                        let last_elem = after.len() - 1;
                        after[last_elem] = '…'
                    }

                    (
                        line_num,
                        before.into_iter().collect(),
                        this.clone(),
                        after.into_iter().collect(),
                    )
                } else {
                    // If we can't even fit the whole important part,
                    // we'll prefer the line number
                    let from_line_num = line_num.len();
                    let from_this = char_budget.saturating_sub(from_line_num);
                    let this = this
                        .chars()
                        .take(from_this)
                        .enumerate()
                        .map(|(i, c)| if i == from_this - 1 { '…' } else { c })
                        .collect();
                    (line_num, "".to_string(), this, "".to_string())
                };

            layout_job.append(
                &line_num,
                0.0,
                TextFormat {
                    font_id: font.clone(),
                    color: foreground.gamma_multiply(0.75),
                    ..Default::default()
                },
            );
            layout_job.append(
                &before,
                0.0,
                TextFormat {
                    font_id: font.clone(),
                    color: foreground.gamma_multiply(0.5),
                    ..Default::default()
                },
            );
            layout_job.append(
                &this,
                0.0,
                TextFormat {
                    font_id: font.clone(),
                    color: foreground,
                    ..Default::default()
                },
            );
            layout_job.append(
                after.trim_end(),
                0.0,
                TextFormat {
                    font_id: font.clone(),
                    color: foreground.gamma_multiply(0.5),
                    ..Default::default()
                },
            )
        }
    }
}
