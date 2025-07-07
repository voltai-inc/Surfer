use std::{
    collections::{HashMap, HashSet, VecDeque},
    mem,
    path::PathBuf,
};

use crate::{
    clock_highlighting::ClockHighlightType,
    config::{ArrowKeyBindings, AutoLoad, PrimaryMouseDrag, SurferConfig},
    data_container::DataContainer,
    dialog::{OpenSiblingStateFileDialog, ReloadWaveformDialog},
    displayed_item_tree::{DisplayedItemTree, VisibleItemIndex},
    hierarchy::HierarchyStyle,
    message::Message,
    system_state::SystemState,
    time::{TimeStringFormatting, TimeUnit},
    transaction_container::TransactionContainer,
    variable_filter::VariableFilter,
    viewport::Viewport,
    wave_container::{ScopeRef, VariableRef, WaveContainer},
    wave_data::WaveData,
    wave_source::{LoadOptions, WaveFormat, WaveSource},
    CanvasState, StartupParams,
};
use egui::{
    style::{Selection, WidgetVisuals, Widgets},
    CornerRadius, Stroke, Visuals,
};
use itertools::Itertools;
use log::{error, info, trace, warn};
use serde::{Deserialize, Serialize};
use surfer_translation_types::Translator;

/// The parts of the program state that need to be serialized when loading/saving state
#[derive(Serialize, Deserialize)]
pub struct UserState {
    #[serde(skip)]
    pub config: SurferConfig,

    /// Overrides for the config show_* fields. Defaults to `config.show_*` if not present
    pub(crate) show_hierarchy: Option<bool>,
    pub(crate) show_menu: Option<bool>,
    pub(crate) show_ticks: Option<bool>,
    pub(crate) show_toolbar: Option<bool>,
    pub(crate) show_tooltip: Option<bool>,
    pub(crate) show_scope_tooltip: Option<bool>,
    pub(crate) show_default_timeline: Option<bool>,
    pub(crate) show_overview: Option<bool>,
    pub(crate) show_statusbar: Option<bool>,
    pub(crate) align_names_right: Option<bool>,
    pub(crate) show_variable_indices: Option<bool>,
    pub(crate) show_variable_direction: Option<bool>,
    pub(crate) show_empty_scopes: Option<bool>,
    pub(crate) show_parameters_in_scopes: Option<bool>,
    #[serde(default)]
    pub(crate) highlight_focused: Option<bool>,
    #[serde(default)]
    pub(crate) fill_high_values: Option<bool>,
    #[serde(default)]
    pub(crate) primary_button_drag_behavior: Option<PrimaryMouseDrag>,
    #[serde(default)]
    pub(crate) arrow_key_bindings: Option<ArrowKeyBindings>,
    #[serde(default)]
    pub(crate) clock_highlight_type: Option<ClockHighlightType>,
    #[serde(default)]
    pub(crate) hierarchy_style: Option<HierarchyStyle>,
    #[serde(default)]
    pub(crate) autoload_sibling_state_files: Option<AutoLoad>,
    #[serde(default)]
    pub(crate) autoreload_files: Option<AutoLoad>,

    pub(crate) waves: Option<WaveData>,
    pub(crate) drag_started: bool,
    pub(crate) drag_source_idx: Option<VisibleItemIndex>,
    pub(crate) drag_target_idx: Option<crate::displayed_item_tree::TargetPosition>,

    pub(crate) previous_waves: Option<WaveData>,

    /// Count argument for movements
    pub(crate) count: Option<String>,

    // Vector of translators which have failed at the `translates` function for a variable.
    pub(crate) blacklisted_translators: HashSet<(VariableRef, String)>,

    pub(crate) show_about: bool,
    pub(crate) show_keys: bool,
    pub(crate) show_gestures: bool,
    pub(crate) show_quick_start: bool,
    pub(crate) show_license: bool,
    pub(crate) show_performance: bool,
    pub(crate) show_logs: bool,
    pub(crate) show_cursor_window: bool,
    pub(crate) wanted_timeunit: TimeUnit,
    pub(crate) time_string_format: Option<TimeStringFormatting>,
    pub(crate) show_url_entry: bool,
    /// Show a confirmation dialog asking the user for confirmation
    /// that surfer should reload changed files from disk.
    #[serde(skip, default)]
    pub(crate) show_reload_suggestion: Option<ReloadWaveformDialog>,
    #[serde(skip, default)]
    pub(crate) show_open_sibling_state_file_suggestion: Option<OpenSiblingStateFileDialog>,
    pub(crate) variable_name_filter_focused: bool,
    pub(crate) variable_filter: VariableFilter,
    pub(crate) rename_target: Option<VisibleItemIndex>,
    //Sidepanel width
    pub(crate) sidepanel_width: Option<f32>,
    /// UI zoom factor if set by the user
    pub(crate) ui_zoom_factor: Option<f32>,

    // Path of last saved-to state file
    // Do not serialize as this causes a few issues and doesn't help:
    // - We need to set it on load of a state anyways since the file could have been renamed
    // - Bad interoperatility story between native and wasm builds
    // - Sequencing issue in serialization, due to us having to run that async
    #[serde(skip)]
    pub state_file: Option<PathBuf>,
}

// Impl needed since for loading we need to put State into a Message
// Snip out the actual contents to not completely spam the terminal
impl std::fmt::Debug for UserState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SharedState {{ <snipped> }}")
    }
}

impl SystemState {
    pub fn with_params(mut self, args: StartupParams) -> Self {
        self.user.previous_waves = self.user.waves;
        self.user.waves = None;

        // we turn the waveform argument and any startup command file into batch commands
        self.batch_messages = VecDeque::new();

        match args.waves {
            Some(WaveSource::Url(url)) => {
                self.add_batch_message(Message::LoadWaveformFileFromUrl(url, LoadOptions::clean()));
            }
            Some(WaveSource::File(file)) => {
                self.add_batch_message(Message::LoadFile(file, LoadOptions::clean()));
            }
            Some(WaveSource::Data) => error!("Attempted to load data at startup"),
            Some(WaveSource::Cxxrtl(url)) => {
                self.add_batch_message(Message::SetupCxxrtl(url));
            }
            Some(WaveSource::DragAndDrop(_)) => {
                error!("Attempted to load from drag and drop at startup (how?)");
            }
            None => {}
        }

        if let Some(port) = args.wcp_initiate {
            let addr = format!("127.0.0.1:{}", port);
            self.add_batch_message(Message::StartWcpServer {
                address: Some(addr),
                initiate: true,
            });
        }

        self.add_batch_commands(args.startup_commands);

        self
    }

    pub fn wcp(&mut self) {
        self.handle_wcp_commands();
    }

    pub(crate) fn get_scope(&mut self, scope: ScopeRef, recursive: bool) -> Vec<VariableRef> {
        let Some(waves) = self.user.waves.as_mut() else {
            return vec![];
        };

        let wave_cont = waves.inner.as_waves().unwrap();

        let children = wave_cont.child_scopes(&scope);
        let mut variables = wave_cont
            .variables_in_scope(&scope)
            .iter()
            .sorted_by(|a, b| numeric_sort::cmp(&a.name, &b.name))
            .cloned()
            .collect_vec();

        if recursive {
            if let Ok(children) = children {
                for child in children {
                    variables.append(&mut self.get_scope(child, true));
                }
            }
        }

        variables
    }

    pub(crate) fn on_waves_loaded(
        &mut self,
        filename: WaveSource,
        format: WaveFormat,
        new_waves: Box<WaveContainer>,
        load_options: LoadOptions,
    ) {
        info!("{format} file loaded");
        let viewport = Viewport::new();
        let viewports = [viewport].to_vec();

        for translator in self.translators.all_translators() {
            translator.set_wave_source(Some(filename.into_translation_type()));
        }

        let ((new_wave, load_commands), is_reload) =
            if load_options.keep_variables && self.user.waves.is_some() {
                (
                    self.user.waves.take().unwrap().update_with_waves(
                        new_waves,
                        filename,
                        format,
                        &self.translators,
                        load_options.keep_unavailable,
                    ),
                    true,
                )
            } else if let Some(old) = self.user.previous_waves.take() {
                (
                    old.update_with_waves(
                        new_waves,
                        filename,
                        format,
                        &self.translators,
                        load_options.keep_unavailable,
                    ),
                    true,
                )
            } else {
                (
                    (
                        WaveData {
                            inner: DataContainer::Waves(*new_waves),
                            source: filename,
                            format,
                            active_scope: None,
                            items_tree: DisplayedItemTree::default(),
                            displayed_items: HashMap::new(),
                            viewports,
                            cursor: None,
                            markers: HashMap::new(),
                            focused_item: None,
                            focused_transaction: (None, None),
                            default_variable_name_type: self.user.config.default_variable_name_type,
                            display_variable_indices: self.show_variable_indices(),
                            scroll_offset: 0.,
                            drawing_infos: vec![],
                            top_item_draw_offset: 0.,
                            total_height: 0.,
                            display_item_ref_counter: 0,
                            old_num_timestamps: None,
                            graphics: HashMap::new(),
                        },
                        None,
                    ),
                    false,
                )
            };

        if let Some(cmd) = load_commands {
            self.load_variables(cmd);
        }
        self.invalidate_draw_commands();

        self.user.waves = Some(new_wave);

        if !is_reload {
            if let Some(waves) = &mut self.user.waves {
                // Set time unit
                self.user.wanted_timeunit = waves.inner.metadata().timescale.unit;
                // Possibly open state file load dialog
                if waves.source.sibling_state_file().is_some() {
                    self.update(Message::SuggestOpenSiblingStateFile);
                }
            }
        }
    }

    pub(crate) fn on_transaction_streams_loaded(
        &mut self,
        filename: WaveSource,
        format: WaveFormat,
        new_ftr: TransactionContainer,
        _loaded_options: LoadOptions,
    ) {
        info!("Transaction streams are loaded.");

        let viewport = Viewport::new();
        let viewports = [viewport].to_vec();

        let new_transaction_streams = WaveData {
            inner: DataContainer::Transactions(new_ftr),
            source: filename,
            format,
            active_scope: None,
            items_tree: DisplayedItemTree::default(),
            displayed_items: HashMap::new(),
            viewports,
            cursor: None,
            markers: HashMap::new(),
            focused_item: None,
            focused_transaction: (None, None),
            default_variable_name_type: self.user.config.default_variable_name_type,
            display_variable_indices: self.show_variable_indices(),
            scroll_offset: 0.,
            drawing_infos: vec![],
            top_item_draw_offset: 0.,
            total_height: 0.,
            display_item_ref_counter: 0,
            old_num_timestamps: None,
            graphics: HashMap::new(),
        };

        self.invalidate_draw_commands();

        self.user.config.theme.alt_frequency = 0;
        self.user.wanted_timeunit = new_transaction_streams.inner.metadata().timescale.unit;
        self.user.waves = Some(new_transaction_streams);
    }

    pub(crate) fn handle_async_messages(&mut self) {
        let mut msgs = vec![];
        loop {
            match self.channels.msg_receiver.try_recv() {
                Ok(msg) => msgs.push(msg),
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    trace!("Message sender disconnected");
                    break;
                }
            }
        }

        while let Some(msg) = msgs.pop() {
            self.update(msg);
        }
    }

    pub fn get_visuals(&self) -> Visuals {
        let widget_style = WidgetVisuals {
            bg_fill: self.user.config.theme.secondary_ui_color.background,
            fg_stroke: Stroke {
                color: self.user.config.theme.secondary_ui_color.foreground,
                width: 1.0,
            },
            weak_bg_fill: self.user.config.theme.secondary_ui_color.background,
            bg_stroke: Stroke {
                color: self.user.config.theme.border_color,
                width: 1.0,
            },
            corner_radius: CornerRadius::same(2),
            expansion: 0.0,
        };

        Visuals {
            override_text_color: Some(self.user.config.theme.foreground),
            extreme_bg_color: self.user.config.theme.secondary_ui_color.background,
            panel_fill: self.user.config.theme.secondary_ui_color.background,
            window_fill: self.user.config.theme.primary_ui_color.background,
            window_stroke: Stroke {
                width: 1.0,
                color: self.user.config.theme.border_color,
            },
            selection: Selection {
                bg_fill: self.user.config.theme.selected_elements_colors.background,
                stroke: Stroke {
                    color: self.user.config.theme.selected_elements_colors.foreground,
                    width: 1.0,
                },
            },
            widgets: Widgets {
                noninteractive: widget_style,
                inactive: widget_style,
                hovered: widget_style,
                active: widget_style,
                open: widget_style,
            },
            ..Visuals::dark()
        }
    }

    pub(crate) fn load_state(&mut self, mut loaded_state: Box<UserState>, path: Option<PathBuf>) {
        // first swap everything, fix special cases afterwards
        mem::swap(&mut self.user, &mut loaded_state);

        // swap back waves for inner, source, format since we want to keep the file
        // fix up all wave references from paths if a wave is loaded
        mem::swap(&mut loaded_state.waves, &mut self.user.waves);
        let load_commands = if let (Some(waves), Some(new_waves)) =
            (&mut self.user.waves, &mut loaded_state.waves)
        {
            mem::swap(&mut waves.active_scope, &mut new_waves.active_scope);
            let items = std::mem::take(&mut new_waves.displayed_items);
            let items_tree = std::mem::take(&mut new_waves.items_tree);
            let load_commands = waves.update_with_items(&items, items_tree, &self.translators);

            mem::swap(&mut waves.viewports, &mut new_waves.viewports);
            mem::swap(&mut waves.cursor, &mut new_waves.cursor);
            mem::swap(&mut waves.markers, &mut new_waves.markers);
            mem::swap(&mut waves.focused_item, &mut new_waves.focused_item);
            waves.default_variable_name_type = new_waves.default_variable_name_type;
            waves.scroll_offset = new_waves.scroll_offset;
            load_commands
        } else {
            None
        };
        if let Some(load_commands) = load_commands {
            self.load_variables(load_commands);
        };

        // reset drag to avoid confusion
        self.user.drag_started = false;
        self.user.drag_source_idx = None;
        self.user.drag_target_idx = None;

        // reset previous_waves & count to prevent unintuitive state here
        self.user.previous_waves = None;
        self.user.count = None;

        // use just loaded path since path is not part of the export as it might have changed anyways
        self.user.state_file = path;
        self.user.rename_target = None;

        self.invalidate_draw_commands();
        if let Some(waves) = &mut self.user.waves {
            waves.update_viewports();
        }
    }

    /// Returns true if the waveform and all requested signals have been loaded.
    /// Used for testing to make sure the GUI is at its final state before taking a
    /// snapshot.
    pub fn waves_fully_loaded(&self) -> bool {
        self.user
            .waves
            .as_ref()
            .is_some_and(|w| w.inner.is_fully_loaded())
    }

    /// Returns the current canvas state
    pub(crate) fn current_canvas_state(waves: &WaveData, message: String) -> CanvasState {
        CanvasState {
            message,
            focused_item: waves.focused_item,
            focused_transaction: waves.focused_transaction.clone(),
            items_tree: waves.items_tree.clone(),
            displayed_items: waves.displayed_items.clone(),
            markers: waves.markers.clone(),
        }
    }

    /// Push the current canvas state to the undo stack
    pub(crate) fn save_current_canvas(&mut self, message: String) {
        if let Some(waves) = &self.user.waves {
            self.undo_stack
                .push(SystemState::current_canvas_state(waves, message));

            if self.undo_stack.len() > self.user.config.undo_stack_size {
                self.undo_stack.remove(0);
            }
            self.redo_stack.clear();
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn start_wcp_server(&mut self, address: Option<String>, initiate: bool) {
        use wcp::wcp_server::WcpServer;

        use crate::wcp;

        if self.wcp_server_thread.as_ref().is_some()
            || self
                .wcp_running_signal
                .load(std::sync::atomic::Ordering::Relaxed)
        {
            warn!("WCP HTTP server is already running");
            return;
        }
        // TODO: Consider an unbounded channel?
        let (wcp_s2c_sender, wcp_s2c_receiver) = tokio::sync::mpsc::channel(100);
        let (wcp_c2s_sender, wcp_c2s_receiver) = tokio::sync::mpsc::channel(100);

        self.channels.wcp_c2s_receiver = Some(wcp_c2s_receiver);
        self.channels.wcp_s2c_sender = Some(wcp_s2c_sender);
        let stop_signal_copy = self.wcp_stop_signal.clone();
        stop_signal_copy.store(false, std::sync::atomic::Ordering::Relaxed);
        let running_signal_copy = self.wcp_running_signal.clone();
        running_signal_copy.store(true, std::sync::atomic::Ordering::Relaxed);
        let greeted_signal_copy = self.wcp_greeted_signal.clone();
        greeted_signal_copy.store(true, std::sync::atomic::Ordering::Relaxed);

        let ctx = self.context.clone();
        let address = address.unwrap_or(self.user.config.wcp.address.clone());
        self.wcp_server_address = Some(address.clone());
        self.wcp_server_thread = Some(tokio::spawn(async move {
            let server = WcpServer::new(
                address,
                initiate,
                wcp_c2s_sender,
                wcp_s2c_receiver,
                stop_signal_copy,
                running_signal_copy,
                greeted_signal_copy,
                ctx,
            )
            .await;
            match server {
                Ok(mut server) => server.run().await,
                Err(m) => {
                    error!("Could not start WCP server. {m:?}")
                }
            }
        }));
    }

    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    pub(crate) fn stop_wcp_server(&mut self) {
        // stop wcp server if there is one running

        if self.wcp_server_address.is_some() && self.wcp_server_thread.is_some() {
            // signal the server to stop
            self.wcp_stop_signal
                .store(true, std::sync::atomic::Ordering::Relaxed);

            self.wcp_server_thread = None;
            self.wcp_server_address = None;
            self.channels.wcp_s2c_sender = None;
            self.channels.wcp_c2s_receiver = None;
            info!("Stopped WCP server");
        }
    }
}
