//! Menu handling.
use egui::{menu, Button, Context, TextWrapMode, TopBottomPanel, Ui};
use eyre::WrapErr;
use futures::executor::block_on;
use itertools::Itertools;
use std::sync::atomic::Ordering;
use surfer_translation_types::{TranslationPreference, Translator};

use crate::config::PrimaryMouseDrag;
use crate::displayed_item_tree::VisibleItemIndex;
use crate::hierarchy::HierarchyStyle;
use crate::message::MessageTarget;
use crate::wave_container::{FieldRef, VariableRefExt};
use crate::wave_source::LoadOptions;
use crate::wcp::{proto::WcpEvent, proto::WcpSCMessage};
use crate::{
    clock_highlighting::clock_highlight_type_menu,
    config::ArrowKeyBindings,
    displayed_item::{DisplayedFieldRef, DisplayedItem},
    file_dialog::OpenMode,
    message::Message,
    time::{timeformat_menu, timeunit_menu},
    variable_name_type::VariableNameType,
    SystemState,
};

// Button builder. Short name because we use it a ton
struct ButtonBuilder {
    text: String,
    shortcut: Option<String>,
    message: Message,
    enabled: bool,
}

impl ButtonBuilder {
    fn new(text: impl Into<String>, message: Message) -> Self {
        Self {
            text: text.into(),
            message,
            shortcut: None,
            enabled: true,
        }
    }

    fn shortcut(mut self, shortcut: impl Into<String>) -> Self {
        self.shortcut = Some(shortcut.into());
        self
    }

    #[cfg_attr(not(feature = "python"), allow(dead_code))]
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    pub fn add_leave_menu(self, msgs: &mut Vec<Message>, ui: &mut Ui) {
        self.add_inner(false, msgs, ui);
    }

    pub fn add_closing_menu(self, msgs: &mut Vec<Message>, ui: &mut Ui) {
        self.add_inner(true, msgs, ui);
    }

    pub fn add_inner(self, close_menu: bool, msgs: &mut Vec<Message>, ui: &mut Ui) {
        let button = Button::new(self.text);
        let button = if let Some(s) = self.shortcut {
            button.shortcut_text(s)
        } else {
            button
        };
        if ui.add_enabled(self.enabled, button).clicked() {
            msgs.push(self.message);
            if close_menu {
                ui.close_menu();
            }
        }
    }
}

impl SystemState {
    pub fn add_menu_panel(&self, ctx: &Context, msgs: &mut Vec<Message>) {
        TopBottomPanel::top("menu").show(ctx, |ui| {
            menu::bar(ui, |ui| {
                self.menu_contents(ui, msgs);
            });
        });
    }

    pub fn menu_contents(&self, ui: &mut Ui, msgs: &mut Vec<Message>) {
        /// Helper function to get a new ButtonBuilder.
        fn b(text: impl Into<String>, message: Message) -> ButtonBuilder {
            ButtonBuilder::new(text, message)
        }

        let waves_loaded = self.user.waves.is_some();

        ui.menu_button("File", |ui| {
            b("Open file...", Message::OpenFileDialog(OpenMode::Open)).add_closing_menu(msgs, ui);
            b("Switch file...", Message::OpenFileDialog(OpenMode::Switch))
                .add_closing_menu(msgs, ui);
            b(
                "Reload",
                Message::ReloadWaveform(self.user.config.behavior.keep_during_reload),
            )
            .shortcut("r")
            .enabled(self.user.waves.is_some())
            .add_closing_menu(msgs, ui);

            b("Load state...", Message::LoadStateFile(None)).add_closing_menu(msgs, ui);
            #[cfg(not(target_arch = "wasm32"))]
            {
                let save_text = if self.user.state_file.is_some() {
                    "Save state"
                } else {
                    "Save state..."
                };
                b(
                    save_text,
                    Message::SaveStateFile(self.user.state_file.clone()),
                )
                .shortcut("Ctrl+s")
                .add_closing_menu(msgs, ui);
            }
            b("Save state as...", Message::SaveStateFile(None)).add_closing_menu(msgs, ui);
            b(
                "Open URL...",
                Message::SetUrlEntryVisible(
                    true,
                    Some(Box::new(|url: String| {
                        Message::LoadWaveformFileFromUrl(url.clone(), LoadOptions::clean())
                    })),
                ),
            )
            .add_closing_menu(msgs, ui);
            #[cfg(target_arch = "wasm32")]
            b("Run command file...", Message::OpenCommandFileDialog)
                .enabled(waves_loaded)
                .add_closing_menu(msgs, ui);
            #[cfg(not(target_arch = "wasm32"))]
            b("Run command file...", Message::OpenCommandFileDialog).add_closing_menu(msgs, ui);
            b(
                "Run command file from URL...",
                Message::SetUrlEntryVisible(
                    true,
                    Some(Box::new(|url: String| {
                        Message::LoadCommandFileFromUrl(url.clone())
                    })),
                ),
            )
            .add_closing_menu(msgs, ui);

            #[cfg(feature = "python")]
            {
                b("Add Python translator", Message::OpenPythonPluginDialog)
                    .add_closing_menu(msgs, ui);
                b("Reload Python translator", Message::ReloadPythonPlugin)
                    .enabled(self.translators.has_python_translator())
                    .add_closing_menu(msgs, ui);
            }
            #[cfg(not(target_arch = "wasm32"))]
            b("Exit", Message::Exit).add_closing_menu(msgs, ui);
        });
        ui.menu_button("View", |ui: &mut Ui| {
            ui.style_mut().wrap_mode = Some(TextWrapMode::Extend);
            b(
                "Zoom in",
                Message::CanvasZoom {
                    mouse_ptr: None,
                    delta: 0.5,
                    viewport_idx: 0,
                },
            )
            .shortcut("+")
            .enabled(waves_loaded)
            .add_leave_menu(msgs, ui);

            b(
                "Zoom out",
                Message::CanvasZoom {
                    mouse_ptr: None,
                    delta: 2.0,
                    viewport_idx: 0,
                },
            )
            .shortcut("-")
            .enabled(waves_loaded)
            .add_leave_menu(msgs, ui);

            b("Zoom to fit", Message::ZoomToFit { viewport_idx: 0 })
                .enabled(waves_loaded)
                .add_closing_menu(msgs, ui);

            ui.separator();

            b("Go to start", Message::GoToStart { viewport_idx: 0 })
                .shortcut("s")
                .enabled(waves_loaded)
                .add_closing_menu(msgs, ui);
            b("Go to end", Message::GoToEnd { viewport_idx: 0 })
                .shortcut("e")
                .enabled(waves_loaded)
                .add_closing_menu(msgs, ui);
            ui.separator();
            b("Add viewport", Message::AddViewport)
                .enabled(waves_loaded)
                .add_closing_menu(msgs, ui);
            b("Remove viewport", Message::RemoveViewport)
                .enabled(waves_loaded)
                .add_closing_menu(msgs, ui);
            ui.separator();

            b("Toggle side panel", Message::ToggleSidePanel)
                .shortcut("b")
                .add_closing_menu(msgs, ui);
            b("Toggle menu", Message::ToggleMenu)
                .shortcut("Alt+m")
                .add_closing_menu(msgs, ui);
            b("Toggle toolbar", Message::ToggleToolbar)
                .shortcut("t")
                .add_closing_menu(msgs, ui);
            b("Toggle overview", Message::ToggleOverview).add_closing_menu(msgs, ui);
            b("Toggle statusbar", Message::ToggleStatusbar).add_closing_menu(msgs, ui);
            b("Toggle timeline", Message::ToggleDefaultTimeline).add_closing_menu(msgs, ui);
            #[cfg(not(target_arch = "wasm32"))]
            b("Toggle full screen", Message::ToggleFullscreen)
                .shortcut("F11")
                .add_closing_menu(msgs, ui);
            ui.menu_button("Theme", |ui| {
                ui.style_mut().wrap_mode = Some(TextWrapMode::Extend);
                b("Default theme", Message::SelectTheme(None)).add_closing_menu(msgs, ui);
                for theme_name in self.user.config.theme.theme_names.clone() {
                    b(theme_name.clone(), Message::SelectTheme(Some(theme_name)))
                        .add_closing_menu(msgs, ui);
                }
            });
            ui.menu_button("UI zoom factor", |ui| {
                ui.style_mut().wrap_mode = Some(TextWrapMode::Extend);
                for scale in &self.user.config.layout.zoom_factors {
                    ui.radio(
                        self.ui_zoom_factor() == *scale,
                        format!("{} %", scale * 100.),
                    )
                    .clicked()
                    .then(|| {
                        ui.close_menu();
                        msgs.push(Message::SetUIZoomFactor(*scale))
                    });
                }
            });
        });

        ui.menu_button("Settings", |ui| {
            ui.menu_button("Clock highlighting", |ui| {
                clock_highlight_type_menu(ui, msgs, self.clock_highlight_type());
            });
            ui.menu_button("Time unit", |ui| {
                timeunit_menu(ui, msgs, &self.user.wanted_timeunit);
            });
            ui.menu_button("Time format", |ui| {
                timeformat_menu(ui, msgs, &self.get_time_format());
            });
            if let Some(waves) = &self.user.waves {
                let variable_name_type = waves.default_variable_name_type;
                ui.menu_button("Variable names", |ui| {
                    for name_type in enum_iterator::all::<VariableNameType>() {
                        ui.radio(variable_name_type == name_type, name_type.to_string())
                            .clicked()
                            .then(|| {
                                ui.close_menu();
                                msgs.push(Message::ForceVariableNameTypes(name_type));
                            });
                    }
                });
            }
            ui.menu_button("Variable name alignment", |ui| {
                let align_right = self
                    .user
                    .align_names_right
                    .unwrap_or_else(|| self.user.config.layout.align_names_right());
                ui.radio(!align_right, "Left").clicked().then(|| {
                    ui.close_menu();
                    msgs.push(Message::SetNameAlignRight(false));
                });
                ui.radio(align_right, "Right").clicked().then(|| {
                    ui.close_menu();
                    msgs.push(Message::SetNameAlignRight(true));
                });
            });
            ui.menu_button("Variable filter type", |ui| {
                self.variable_filter_type_menu(ui, msgs);
            });

            ui.menu_button("Hierarchy", |ui| {
                for style in enum_iterator::all::<HierarchyStyle>() {
                    ui.radio(self.hierarchy_style() == style, style.to_string())
                        .clicked()
                        .then(|| {
                            ui.close_menu();
                            msgs.push(Message::SetHierarchyStyle(style));
                        });
                }
            });

            ui.menu_button("Arrow keys", |ui| {
                for binding in enum_iterator::all::<ArrowKeyBindings>() {
                    ui.radio(self.arrow_key_bindings() == binding, binding.to_string())
                        .clicked()
                        .then(|| {
                            ui.close_menu();
                            msgs.push(Message::SetArrowKeyBindings(binding));
                        });
                }
            });

            ui.menu_button("Primary mouse button drag", |ui| {
                for behavior in enum_iterator::all::<PrimaryMouseDrag>() {
                    ui.radio(
                        self.primary_button_drag_behavior() == behavior,
                        behavior.to_string(),
                    )
                    .clicked()
                    .then(|| {
                        ui.close_menu();
                        msgs.push(Message::SetPrimaryMouseDragBehavior(behavior));
                    });
                }
            });

            ui.radio(self.show_ticks(), "Show tick lines")
                .clicked()
                .then(|| {
                    ui.close_menu();
                    msgs.push(Message::ToggleTickLines);
                });

            ui.radio(self.show_tooltip(), "Show variable tooltip")
                .clicked()
                .then(|| {
                    ui.close_menu();
                    msgs.push(Message::ToggleVariableTooltip);
                });

            ui.radio(self.show_scope_tooltip(), "Show scope tooltip")
                .clicked()
                .then(|| {
                    ui.close_menu();
                    msgs.push(Message::ToggleScopeTooltip);
                });

            ui.radio(self.show_variable_indices(), "Show variable indices")
                .clicked()
                .then(|| {
                    ui.close_menu();
                    msgs.push(Message::ToggleIndices);
                });

            ui.radio(self.show_variable_direction(), "Show variable direction")
                .clicked()
                .then(|| {
                    ui.close_menu();
                    msgs.push(Message::ToggleDirection);
                });

            ui.radio(self.show_empty_scopes(), "Show empty scopes")
                .clicked()
                .then(|| {
                    ui.close_menu();
                    msgs.push(Message::ToggleEmptyScopes);
                });

            ui.radio(
                self.show_parameters_in_scopes(),
                "Show parameters in scopes",
            )
            .clicked()
            .then(|| {
                ui.close_menu();
                msgs.push(Message::ToggleParametersInScopes);
            });

            ui.radio(self.highlight_focused(), "Highlight focused")
                .clicked()
                .then(|| {
                    ui.close_menu();
                    msgs.push(Message::SetHighlightFocused(!self.highlight_focused()))
                });

            ui.radio(self.fill_high_values(), "Fill high values")
                .clicked()
                .then(|| {
                    ui.close_menu();
                    msgs.push(Message::SetFillHighValues(!self.fill_high_values()));
                });
        });
        ui.menu_button("Help", |ui| {
            b("Quick start", Message::SetQuickStartVisible(true)).add_closing_menu(msgs, ui);
            b("Control keys", Message::SetKeyHelpVisible(true)).add_closing_menu(msgs, ui);
            b("Mouse gestures", Message::SetGestureHelpVisible(true)).add_closing_menu(msgs, ui);

            ui.separator();
            b("Show logs", Message::SetLogsVisible(true)).add_closing_menu(msgs, ui);

            ui.separator();
            b("License information", Message::SetLicenseVisible(true)).add_closing_menu(msgs, ui);
            ui.separator();
            b("About", Message::SetAboutVisible(true)).add_closing_menu(msgs, ui);
        });
    }

    pub fn item_context_menu(
        &self,
        path: Option<&FieldRef>,
        msgs: &mut Vec<Message>,
        ui: &mut Ui,
        vidx: VisibleItemIndex,
    ) {
        let Some(waves) = &self.user.waves else {
            return;
        };

        let (displayed_item_id, displayed_item) = waves
            .items_tree
            .get_visible(vidx)
            .map(|node| (node.item_ref, &waves.displayed_items[&node.item_ref]))
            .unwrap();

        let affect_selected = waves
            .items_tree
            .iter_visible_selected()
            .map(|node| node.item_ref)
            .contains(&displayed_item_id);
        let affected_vidxs = if affect_selected { None } else { Some(vidx) };

        if let Some(path) = path {
            let dfr = DisplayedFieldRef {
                item: displayed_item_id,
                field: path.field.clone(),
            };
            self.add_format_menu(&dfr, displayed_item, path, msgs, ui);
        }

        ui.menu_button("Color", |ui| {
            let selected_color = &displayed_item.color().unwrap_or("__nocolor__");
            for color_name in self.user.config.theme.colors.keys() {
                ui.radio(selected_color == color_name, color_name)
                    .clicked()
                    .then(|| {
                        ui.close_menu();
                        msgs.push(Message::ItemColorChange(
                            affected_vidxs.into(),
                            Some(color_name.clone()),
                        ));
                    });
            }
            ui.separator();
            ui.radio(*selected_color == "__nocolor__", "Default")
                .clicked()
                .then(|| {
                    ui.close_menu();
                    msgs.push(Message::ItemColorChange(
                        MessageTarget::Explicit(vidx),
                        None,
                    ));
                });
        });

        ui.menu_button("Background color", |ui| {
            let selected_color = &displayed_item.background_color().unwrap_or("__nocolor__");
            for color_name in self.user.config.theme.colors.keys() {
                ui.radio(selected_color == color_name, color_name)
                    .clicked()
                    .then(|| {
                        ui.close_menu();
                        msgs.push(Message::ItemBackgroundColorChange(
                            affected_vidxs.into(),
                            Some(color_name.clone()),
                        ));
                    });
            }
            ui.separator();
            ui.radio(*selected_color == "__nocolor__", "Default")
                .clicked()
                .then(|| {
                    ui.close_menu();
                    msgs.push(Message::ItemBackgroundColorChange(
                        MessageTarget::Explicit(vidx),
                        None,
                    ));
                });
        });

        if let DisplayedItem::Variable(variable) = displayed_item {
            // DUMMY BUTTON FOR DEBUGGING - This should always show
            if ui.button("🔍 DEBUG - Test Button").clicked() {
                println!("DEBUG: Test button clicked for variable: {}", variable.variable_ref.name);
                println!("DEBUG: WCP greeted: {}", self.wcp_greeted_signal.load(Ordering::Relaxed));
                ui.close_menu();
            }

            ui.menu_button("Name", |ui| {
                let variable_name_type = variable.display_name_type;
                for name_type in enum_iterator::all::<VariableNameType>() {
                    ui.radio(variable_name_type == name_type, name_type.to_string())
                        .clicked()
                        .then(|| {
                            ui.close_menu();
                            msgs.push(Message::ChangeVariableNameType(
                                MessageTarget::Explicit(vidx),
                                name_type,
                            ));
                        });
                }
            });

            ui.menu_button("Height", |ui| {
                let selected_size = displayed_item.height_scaling_factor();
                for size in &self.user.config.layout.waveforms_line_height_multiples {
                    ui.radio(selected_size == *size, format!("{}", size))
                        .clicked()
                        .then(|| {
                            ui.close_menu();
                            msgs.push(Message::ItemHeightScalingFactorChange(
                                affected_vidxs.into(),
                                *size,
                            ));
                        });
                }
            });

            if self.wcp_greeted_signal.load(Ordering::Relaxed) {
                if self.wcp_client_capabilities.goto_declaration
                    && ui.button("Go to declaration").clicked()
                {
                    let variable = variable.variable_ref.full_path_string();
                    self.channels.wcp_s2c_sender.as_ref().map(|ch| {
                        block_on(
                            ch.send(WcpSCMessage::event(WcpEvent::goto_declaration { variable })),
                        )
                    });
                    ui.close_menu();
                }
                if self.wcp_client_capabilities.add_drivers && ui.button("Add drivers").clicked() {
                    let variable = variable.variable_ref.full_path_string();
                    self.channels.wcp_s2c_sender.as_ref().map(|ch| {
                        block_on(ch.send(WcpSCMessage::event(WcpEvent::add_drivers { variable })))
                    });
                    ui.close_menu();
                }
                if self.wcp_client_capabilities.add_loads && ui.button("Add loads").clicked() {
                    let variable = variable.variable_ref.full_path_string();
                    self.channels.wcp_s2c_sender.as_ref().map(|ch| {
                        block_on(ch.send(WcpSCMessage::event(WcpEvent::add_loads { variable })))
                    });
                    ui.close_menu();
                }
                if ui.button("Open Source").clicked() {
                    let signal_name = variable.variable_ref.name.clone();
                    let full_path = variable.variable_ref.full_path_string();
                    msgs.push(Message::OpenSource { signal_name, full_path });
                    ui.close_menu();
                }
            }
        }

        if ui.button("Rename").clicked() {
            ui.close_menu();
            msgs.push(Message::RenameItem(Some(vidx)));
        }

        if ui.button("Remove").clicked() {
            msgs.push(
                if waves
                    .items_tree
                    .iter_visible_selected()
                    .map(|node| node.item_ref)
                    .contains(&displayed_item_id)
                {
                    Message::Batch(vec![
                        Message::RemoveItems(
                            waves
                                .items_tree
                                .iter_visible_selected()
                                .map(|node| node.item_ref)
                                .collect_vec(),
                        ),
                        Message::UnfocusItem,
                    ])
                } else {
                    Message::RemoveItems(vec![displayed_item_id])
                },
            );
            msgs.push(Message::InvalidateCount);
            ui.close_menu();
        }
        if path.is_some() {
            // Actual signal. Not one of: divider, timeline, marker.
            ui.menu_button("Copy", |ui| {
                #[allow(clippy::collapsible_if)]
                if waves.cursor.is_some() {
                    if ui.button("Value").clicked() {
                        ui.close_menu();
                        msgs.push(Message::VariableValueToClipbord(MessageTarget::Explicit(
                            vidx,
                        )));
                    }
                }
                if ui.button("Name").clicked() {
                    ui.close_menu();
                    msgs.push(Message::VariableNameToClipboard(MessageTarget::Explicit(
                        vidx,
                    )));
                }
                if ui.button("Full name").clicked() {
                    ui.close_menu();
                    msgs.push(Message::VariableFullNameToClipboard(
                        MessageTarget::Explicit(vidx),
                    ));
                }
            });
        }
        ui.separator();
        ui.menu_button("Insert", |ui| {
            if ui.button("Divider").clicked() {
                ui.close_menu();
                msgs.push(Message::AddDivider(None, Some(vidx)));
            }
            if ui.button("Timeline").clicked() {
                ui.close_menu();
                msgs.push(Message::AddTimeLine(Some(vidx)));
            }
        });

        ui.menu_button("Group", |ui| {
            let info = waves
                .items_tree
                .iter_visible_extra()
                .find(|info| info.node.item_ref == displayed_item_id)
                .expect("Inconsistent, could not find displayed signal in tree");

            if ui.button("Create").clicked() {
                ui.close_menu();

                let mut items = if affect_selected {
                    waves
                        .items_tree
                        .iter_visible_selected()
                        .map(|node| node.item_ref)
                        .collect::<Vec<_>>()
                } else {
                    vec![]
                };
                // the focused item may not yet be selected, so add it
                if affect_selected {
                    if let Some(focused_item_node) = waves
                        .focused_item
                        .and_then(|focused_item| waves.items_tree.get_visible(focused_item))
                    {
                        items.push(focused_item_node.item_ref);
                    }
                }

                // the clicked item may not be selected yet, add it
                items.push(displayed_item_id);

                msgs.push(Message::GroupNew {
                    name: None,
                    before: Some(info.idx),
                    items: Some(items),
                })
            }
            if matches!(displayed_item, DisplayedItem::Group(_)) {
                if ui.button("Dissolve").clicked() {
                    ui.close_menu();
                    msgs.push(Message::GroupDissolve(Some(displayed_item_id)))
                }

                let (text, msg, msg_recursive) = if info.node.unfolded {
                    (
                        "Collapse",
                        Message::GroupFold(Some(displayed_item_id)),
                        Message::GroupFoldRecursive(Some(displayed_item_id)),
                    )
                } else {
                    (
                        "Expand",
                        Message::GroupUnfold(Some(displayed_item_id)),
                        Message::GroupUnfold(Some(displayed_item_id)),
                    )
                };
                if ui.button(text).clicked() {
                    ui.close_menu();
                    msgs.push(msg)
                }
                if ui.button(text.to_owned() + " recursive").clicked() {
                    ui.close_menu();
                    msgs.push(msg_recursive)
                }
            }
        });
    }

    fn add_format_menu(
        &self,
        displayed_field_ref: &DisplayedFieldRef,
        displayed_item: &DisplayedItem,
        path: &FieldRef,
        msgs: &mut Vec<Message>,
        ui: &mut Ui,
    ) {
        // Should not call this unless a variable is selected, and, hence, a VCD is loaded
        let Some(waves) = &self.user.waves else {
            return;
        };

        let (mut preferred_translators, mut bad_translators) = if path.field.is_empty() {
            self.translators
                .all_translator_names()
                .into_iter()
                .partition(|translator_name| {
                    let t = self.translators.get_translator(translator_name);

                    if self
                        .user
                        .blacklisted_translators
                        .contains(&(path.root.clone(), translator_name.to_string()))
                    {
                        false
                    } else {
                        match waves
                            .inner
                            .as_waves()
                            .unwrap()
                            .variable_meta(&path.root)
                            .and_then(|meta| t.translates(&meta))
                            .context(format!(
                                "Failed to check if {translator_name} translates {:?}",
                                path.root.full_path(),
                            )) {
                            Ok(TranslationPreference::Yes) => true,
                            Ok(TranslationPreference::Prefer) => true,
                            Ok(TranslationPreference::No) => false,
                            Err(e) => {
                                msgs.push(Message::BlacklistTranslator(
                                    path.root.clone(),
                                    translator_name.to_string(),
                                ));
                                msgs.push(Message::Error(e));
                                false
                            }
                        }
                    }
                })
        } else {
            (self.translators.basic_translator_names(), vec![])
        };

        preferred_translators.sort_by(|a, b| numeric_sort::cmp(a, b));
        bad_translators.sort_by(|a, b| numeric_sort::cmp(a, b));

        let selected_translator = match displayed_item {
            DisplayedItem::Variable(var) => Some(var),
            _ => None,
        }
        .and_then(|displayed_variable| displayed_variable.get_format(&displayed_field_ref.field));

        let mut menu_entry = |ui: &mut Ui, name: &str| {
            ui.radio(selected_translator.is_some_and(|st| st == name), name)
                .clicked()
                .then(|| {
                    ui.close_menu();
                    msgs.push(Message::VariableFormatChange(
                        if waves
                            .items_tree
                            .iter_visible_selected()
                            .map(|node| node.item_ref)
                            .contains(&displayed_field_ref.item)
                        {
                            MessageTarget::CurrentSelection
                        } else {
                            MessageTarget::Explicit(displayed_field_ref.clone())
                        },
                        name.to_string(),
                    ));
                });
        };

        ui.menu_button("Format", |ui| {
            for name in preferred_translators {
                menu_entry(ui, name);
            }
            if !bad_translators.is_empty() {
                ui.separator();
                ui.menu_button("Not recommended", |ui| {
                    for name in bad_translators {
                        menu_entry(ui, name);
                    }
                });
            }
        });
    }
}
