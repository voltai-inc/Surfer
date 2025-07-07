//! The items that are drawn in the main wave form view: waves, dividers, etc.
use ecolor::Color32;
use egui::{Context, FontSelection, Key, RichText, Style, WidgetText, Window};
use emath::Align;
use epaint::text::LayoutJob;
use serde::{Deserialize, Serialize};
use surfer_translation_types::VariableInfo;

use crate::config::SurferConfig;
use crate::displayed_item_tree::VisibleItemIndex;
use crate::transaction_container::TransactionStreamRef;
use crate::wave_container::{FieldRef, VariableRef, VariableRefExt, WaveContainer};
use crate::{
    marker::DEFAULT_MARKER_NAME, message::Message, time::DEFAULT_TIMELINE_NAME,
    variable_name_type::VariableNameType,
};

const DEFAULT_DIVIDER_NAME: &str = "";

/// Key for the [`crate::wave_data::WaveData::displayed_items`] hash map
#[derive(Serialize, Deserialize, Debug, Copy, Clone, Eq, PartialEq, Hash, PartialOrd, Ord)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen)]
pub struct DisplayedItemRef(pub usize);

impl From<usize> for DisplayedItemRef {
    fn from(item: usize) -> Self {
        DisplayedItemRef(item)
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Eq, PartialEq, Hash)]
pub struct DisplayedFieldRef {
    pub item: DisplayedItemRef,
    pub field: Vec<String>,
}

impl DisplayedFieldRef {
    pub fn without_field(&self) -> DisplayedFieldRef {
        DisplayedFieldRef {
            item: self.item,
            field: vec![],
        }
    }
}

impl From<DisplayedItemRef> for DisplayedFieldRef {
    fn from(item: DisplayedItemRef) -> Self {
        DisplayedFieldRef {
            item,
            field: vec![],
        }
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub enum DisplayedItem {
    Variable(DisplayedVariable),
    Divider(DisplayedDivider),
    Marker(DisplayedMarker),
    TimeLine(DisplayedTimeLine),
    Placeholder(DisplayedPlaceholder),
    Stream(DisplayedStream),
    Group(DisplayedGroup),
}

#[derive(Serialize, Deserialize, Clone)]
pub struct FieldFormat {
    pub field: Vec<String>,
    pub format: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct DisplayedVariable {
    pub variable_ref: VariableRef,
    #[serde(skip)]
    pub info: VariableInfo,
    pub color: Option<String>,
    pub background_color: Option<String>,
    pub display_name: String,
    pub display_name_type: VariableNameType,
    pub manual_name: Option<String>,
    pub format: Option<String>,
    pub field_formats: Vec<FieldFormat>,
    pub height_scaling_factor: Option<f32>,
}

impl DisplayedVariable {
    pub fn get_format(&self, field: &[String]) -> Option<&String> {
        if field.is_empty() {
            self.format.as_ref()
        } else {
            self.field_formats
                .iter()
                .find(|ff| ff.field == field)
                .map(|ff| &ff.format)
        }
    }

    /// Updates the variable after a new waveform has been loaded.
    pub fn update(
        &self,
        new_waves: &WaveContainer,
        keep_unavailable: bool,
    ) -> Option<DisplayedItem> {
        match new_waves.update_variable_ref(&self.variable_ref) {
            // variable is not available in the new waveform
            None if keep_unavailable => {
                Some(DisplayedItem::Placeholder(self.clone().into_placeholder()))
            }
            None => None,
            Some(new_ref) => {
                let mut res = self.clone();
                res.variable_ref = new_ref;
                Some(DisplayedItem::Variable(res))
            }
        }
    }

    pub fn into_placeholder(mut self) -> DisplayedPlaceholder {
        self.variable_ref.clear_id(); // placeholders do not refer to currently loaded variables
        DisplayedPlaceholder {
            variable_ref: self.variable_ref,
            color: self.color,
            background_color: self.background_color,
            display_name: self.display_name,
            display_name_type: self.display_name_type,
            manual_name: self.manual_name,
            format: self.format,
            field_formats: self.field_formats,
            height_scaling_factor: self.height_scaling_factor,
        }
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct DisplayedDivider {
    pub color: Option<String>,
    pub background_color: Option<String>,
    pub name: Option<String>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct DisplayedMarker {
    pub color: Option<String>,
    pub background_color: Option<String>,
    pub name: Option<String>,
    pub idx: u8,
}

impl DisplayedMarker {
    pub fn marker_text(&self, color: &Color32) -> WidgetText {
        let style = Style::default();
        let mut layout_job = LayoutJob::default();
        self.rich_text(color, &style, &mut layout_job);
        WidgetText::LayoutJob(layout_job)
    }

    pub fn rich_text(&self, color: &Color32, style: &Style, layout_job: &mut LayoutJob) {
        RichText::new(format!("{idx}: ", idx = self.idx))
            .color(*color)
            .append_to(layout_job, style, FontSelection::Default, Align::Center);
        RichText::new(self.marker_name())
            .color(*color)
            .italics()
            .append_to(layout_job, style, FontSelection::Default, Align::Center);
    }

    fn marker_name(&self) -> String {
        self.name
            .as_ref()
            .cloned()
            .unwrap_or_else(|| DEFAULT_MARKER_NAME.to_string())
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct DisplayedTimeLine {
    pub color: Option<String>,
    pub background_color: Option<String>,
    pub name: Option<String>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct DisplayedPlaceholder {
    pub variable_ref: VariableRef,
    pub color: Option<String>,
    pub background_color: Option<String>,
    pub display_name: String,
    pub display_name_type: VariableNameType,
    pub manual_name: Option<String>,
    pub format: Option<String>,
    pub field_formats: Vec<FieldFormat>,
    pub height_scaling_factor: Option<f32>,
}

impl DisplayedPlaceholder {
    pub fn into_variable(
        self,
        variable_info: VariableInfo,
        updated_variable_ref: VariableRef,
    ) -> DisplayedVariable {
        DisplayedVariable {
            variable_ref: updated_variable_ref,
            info: variable_info,
            color: self.color,
            background_color: self.background_color,
            display_name: self.display_name,
            display_name_type: self.display_name_type,
            manual_name: self.manual_name,
            format: self.format,
            field_formats: self.field_formats,
            height_scaling_factor: self.height_scaling_factor,
        }
    }

    pub fn rich_text(&self, text_color: Color32, style: &Style, layout_job: &mut LayoutJob) {
        let s = self.manual_name.as_ref().unwrap_or(&self.display_name);
        RichText::new("Not available: ".to_owned() + s)
            .color(text_color)
            .italics()
            .append_to(layout_job, style, FontSelection::Default, Align::Center);
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct DisplayedStream {
    pub transaction_stream_ref: TransactionStreamRef,
    pub color: Option<String>,
    pub background_color: Option<String>,
    pub display_name: String,
    pub manual_name: Option<String>,
    pub rows: usize,
}

impl DisplayedStream {
    pub fn rich_text(
        &self,
        text_color: Color32,
        style: &Style,
        config: &SurferConfig,
        layout_job: &mut LayoutJob,
    ) {
        RichText::new(format!(
            "{}{}",
            self.manual_name.as_ref().unwrap_or(&self.display_name),
            "\n".repeat(self.rows - 1)
        ))
        .color(text_color)
        // TODO: What does setting this do? Is it for the multi-line transactions?
        .line_height(Some(config.layout.transactions_line_height))
        .append_to(layout_job, style, FontSelection::Default, Align::Center);
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct DisplayedGroup {
    pub name: String,
    pub color: Option<String>,
    pub background_color: Option<String>,
    pub content: Vec<DisplayedItemRef>,
    pub is_open: bool,
}

impl DisplayedGroup {
    pub fn rich_text(&self, text_color: Color32, style: &Style, layout_job: &mut LayoutJob) {
        RichText::new(self.name.clone())
            .color(text_color)
            .append_to(layout_job, style, FontSelection::Default, Align::Center);
    }
}

impl DisplayedItem {
    pub fn color(&self) -> Option<&str> {
        match self {
            DisplayedItem::Variable(variable) => variable.color.as_deref(),
            DisplayedItem::Divider(divider) => divider.color.as_deref(),
            DisplayedItem::Marker(marker) => marker.color.as_deref(),
            DisplayedItem::TimeLine(timeline) => timeline.color.as_deref(),
            DisplayedItem::Placeholder(_) => None,
            DisplayedItem::Stream(stream) => stream.color.as_deref(),
            DisplayedItem::Group(group) => group.color.as_deref(),
        }
    }

    pub fn set_color(&mut self, color_name: Option<String>) {
        match self {
            DisplayedItem::Variable(variable) => variable.color.clone_from(&color_name),
            DisplayedItem::Divider(divider) => divider.color.clone_from(&color_name),
            DisplayedItem::Marker(marker) => marker.color.clone_from(&color_name),
            DisplayedItem::TimeLine(timeline) => timeline.color.clone_from(&color_name),
            DisplayedItem::Placeholder(placeholder) => placeholder.color.clone_from(&color_name),
            DisplayedItem::Stream(stream) => stream.color.clone_from(&color_name),
            DisplayedItem::Group(group) => group.color.clone_from(&color_name),
        }
    }

    pub fn name(&self) -> String {
        match self {
            DisplayedItem::Variable(variable) => variable
                .manual_name
                .as_ref()
                .unwrap_or(&variable.display_name)
                .clone(),
            DisplayedItem::Divider(divider) => divider
                .name
                .as_ref()
                .unwrap_or(&DEFAULT_DIVIDER_NAME.to_string())
                .clone(),
            DisplayedItem::Marker(marker) => marker.marker_name(),
            DisplayedItem::TimeLine(timeline) => timeline
                .name
                .as_ref()
                .unwrap_or(&DEFAULT_TIMELINE_NAME.to_string())
                .clone(),
            DisplayedItem::Placeholder(placeholder) => placeholder
                .manual_name
                .as_ref()
                .unwrap_or(&placeholder.display_name)
                .clone(),
            DisplayedItem::Stream(stream) => stream
                .manual_name
                .as_ref()
                .unwrap_or(&stream.display_name)
                .clone(),
            DisplayedItem::Group(group) => group.name.clone(),
        }
    }

    /// Widget displayed in variable list for the wave form, may include additional info compared to name()
    pub fn add_to_layout_job(
        &self,
        color: &Color32,
        style: &Style,
        layout_job: &mut LayoutJob,
        field: Option<&FieldRef>,
        config: &SurferConfig,
    ) {
        match self {
            DisplayedItem::Variable(_) => {
                let name = if let Some(field) = field {
                    if let Some(last) = field.field.last() {
                        last.clone()
                    } else {
                        self.name()
                    }
                } else {
                    self.name()
                };
                RichText::new(name)
                    .color(*color)
                    .line_height(Some(
                        config.layout.waveforms_line_height * self.height_scaling_factor(),
                    ))
                    .append_to(layout_job, style, FontSelection::Default, Align::Center);
            }
            DisplayedItem::TimeLine(_) | DisplayedItem::Divider(_) => {
                RichText::new(self.name())
                    .color(*color)
                    .italics()
                    .append_to(layout_job, style, FontSelection::Default, Align::Center);
            }
            DisplayedItem::Marker(marker) => {
                marker.rich_text(color, style, layout_job);
            }
            DisplayedItem::Placeholder(placeholder) => {
                let s = placeholder
                    .manual_name
                    .as_ref()
                    .unwrap_or(&placeholder.display_name);
                RichText::new("Not available: ".to_owned() + s)
                    .color(*color)
                    .italics()
                    .append_to(layout_job, style, FontSelection::Default, Align::Center);
            }
            DisplayedItem::Stream(stream) => {
                RichText::new(format!("{}{}", self.name(), "\n".repeat(stream.rows - 1)))
                    .color(*color)
                    .line_height(Some(config.layout.transactions_line_height))
                    .append_to(layout_job, style, FontSelection::Default, Align::Center);
            }
            DisplayedItem::Group(group) => {
                group.rich_text(*color, style, layout_job);
            }
        }
    }

    pub fn set_name(&mut self, name: Option<String>) {
        match self {
            DisplayedItem::Variable(variable) => {
                variable.manual_name = name;
            }
            DisplayedItem::Divider(divider) => {
                divider.name = name;
            }
            DisplayedItem::Marker(marker) => {
                marker.name = name;
            }
            DisplayedItem::TimeLine(timeline) => {
                timeline.name = name;
            }
            DisplayedItem::Placeholder(placeholder) => {
                placeholder.manual_name = name;
            }
            DisplayedItem::Stream(stream) => {
                stream.manual_name = name;
            }
            DisplayedItem::Group(group) => {
                group.name = name.unwrap_or_default();
            }
        }
    }

    pub fn background_color(&self) -> Option<&str> {
        match self {
            DisplayedItem::Variable(variable) => variable.background_color.as_deref(),
            DisplayedItem::Divider(divider) => divider.background_color.as_deref(),
            DisplayedItem::Marker(marker) => marker.background_color.as_deref(),
            DisplayedItem::TimeLine(timeline) => timeline.background_color.as_deref(),
            DisplayedItem::Placeholder(_) => None,
            DisplayedItem::Stream(stream) => stream.background_color.as_deref(),
            DisplayedItem::Group(group) => group.background_color.as_deref(),
        }
    }

    pub fn set_background_color(&mut self, color_name: Option<String>) {
        match self {
            DisplayedItem::Variable(variable) => {
                variable.background_color.clone_from(&color_name);
            }
            DisplayedItem::Divider(divider) => {
                divider.background_color.clone_from(&color_name);
            }
            DisplayedItem::Marker(marker) => {
                marker.background_color.clone_from(&color_name);
            }
            DisplayedItem::TimeLine(timeline) => {
                timeline.background_color.clone_from(&color_name);
            }
            DisplayedItem::Placeholder(placeholder) => {
                placeholder.background_color.clone_from(&color_name);
            }
            DisplayedItem::Stream(stream) => {
                stream.background_color.clone_from(&color_name);
            }
            DisplayedItem::Group(group) => {
                group.background_color.clone_from(&color_name);
            }
        }
    }

    pub fn height_scaling_factor(&self) -> f32 {
        match self {
            DisplayedItem::Variable(variable) => variable.height_scaling_factor,
            DisplayedItem::Placeholder(placeholder) => placeholder.height_scaling_factor,
            _ => None,
        }
        .unwrap_or(1.0)
    }

    pub fn set_height_scaling_factor(&mut self, scale: f32) {
        match self {
            DisplayedItem::Variable(variable) => variable.height_scaling_factor = Some(scale),
            DisplayedItem::Placeholder(placeholder) => {
                placeholder.height_scaling_factor = Some(scale)
            }
            _ => {}
        }
    }
}

pub fn draw_rename_window(
    ctx: &Context,
    msgs: &mut Vec<Message>,
    vidx: VisibleItemIndex,
    name: &mut String,
) {
    let mut open = true;
    Window::new("Rename item")
        .open(&mut open)
        .collapsible(false)
        .resizable(true)
        .show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                let response = ui.text_edit_singleline(name);
                if response.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                    msgs.push(Message::ItemNameChange(Some(vidx), Some(name.clone())));
                    msgs.push(Message::SetRenameItemVisible(false));
                }
                response.request_focus();
                ui.horizontal(|ui| {
                    if ui.button("Rename").clicked() {
                        msgs.push(Message::ItemNameChange(Some(vidx), Some(name.clone())));
                        msgs.push(Message::SetRenameItemVisible(false));
                    }
                    if ui.button("Default").clicked() {
                        msgs.push(Message::ItemNameChange(Some(vidx), None));
                        msgs.push(Message::SetRenameItemVisible(false));
                    }
                    if ui.button("Cancel").clicked() {
                        msgs.push(Message::SetRenameItemVisible(false));
                    }
                });
            });
        });
    if !open {
        msgs.push(Message::SetRenameItemVisible(false));
    }
}
