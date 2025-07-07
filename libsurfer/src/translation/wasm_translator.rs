use std::ffi::OsString;
use std::fs::read_dir;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use camino::Utf8PathBuf;
use directories::ProjectDirs;
use extism::{host_fn, Manifest, Plugin, PluginBuilder, Wasm, PTR};
use extism_manifest::MemoryOptions;
use eyre::{anyhow, Context};
use log::{error, warn};
use surfer_translation_types::plugin_types::TranslateParams;
use surfer_translation_types::{
    TranslationPreference, TranslationResult, Translator, VariableInfo, VariableMeta,
    VariableNameInfo, VariableValue,
};

use crate::message::Message;
use crate::wave_container::{ScopeId, VarId};

pub fn discover_wasm_translators() -> Vec<Message> {
    let search_dirs = [
        std::env::current_dir()
            .ok()
            .map(|dir| dir.join(".surfer").join("translators")),
        ProjectDirs::from("org", "surfer-project", "surfer")
            .map(|dirs| dirs.data_dir().join("translators")),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>();

    let plugin_files = search_dirs
        .into_iter()
        .flat_map(|dir| {
            if !dir.exists() {
                return vec![];
            }
            read_dir(&dir)
                .map(|readdir| {
                    readdir
                        .filter_map(|entry| match entry {
                            Ok(entry) => {
                                let path = entry.path();
                                if path.extension() == Some(&OsString::from("wasm")) {
                                    Some(path)
                                } else {
                                    None
                                }
                            }
                            Err(e) => {
                                warn!("Failed to read entry in {:?}. {e}", dir.to_string_lossy());
                                None
                            }
                        })
                        .collect::<Vec<_>>()
                })
                .map_err(|e| {
                    warn!(
                        "Failed to read dir entries in {}. {e}",
                        dir.to_string_lossy()
                    )
                })
                .unwrap_or_else(|_| vec![])
        })
        .filter_map(|file| {
            file.clone()
                .try_into()
                .map_err(|_| {
                    format!(
                        "{} is not a valid UTF8 path, ignoring this translator",
                        file.to_string_lossy()
                    )
                })
                .ok()
        });

    plugin_files.map(Message::LoadWasmTranslator).collect()
}

pub struct PluginTranslator {
    plugin: Arc<Mutex<Plugin>>,
    file: PathBuf,
}

impl PluginTranslator {
    pub fn new(file: PathBuf) -> eyre::Result<Self> {
        let data = std::fs::read(&file)
            .with_context(|| format!("Failed to read {}", file.to_string_lossy()))?;

        let manifest = Manifest::new([Wasm::data(data)])
            .with_memory_options(MemoryOptions::new().with_max_var_bytes(1024 * 1024 * 10));
        let mut plugin = PluginBuilder::new(manifest)
            .with_function(
                "read_file",
                [PTR],
                [PTR],
                extism::UserData::new(()),
                read_file,
            )
            .with_function(
                "file_exists",
                [PTR],
                [PTR],
                extism::UserData::new(()),
                file_exists,
            )
            .build()
            .map_err(|e| anyhow!("Failed to load plugin from {} {e}", file.to_string_lossy()))?;

        if plugin.function_exists("new") {
            plugin.call::<_, ()>("new", ()).map_err(|e| {
                anyhow!(
                    "Failed to call `new` on plugin from {}. {e}",
                    file.to_string_lossy()
                )
            })?;
        }

        Ok(Self {
            plugin: Arc::new(Mutex::new(plugin)),
            file,
        })
    }
}

impl Translator<VarId, ScopeId, Message> for PluginTranslator {
    fn name(&self) -> String {
        self.plugin
            .lock()
            .unwrap()
            .call::<_, &str>("name", ())
            .map_err(|e| {
                error!(
                    "Failed to get translator name from {}. {e}",
                    self.file.to_string_lossy()
                );
            })
            .map(|s| s.to_string())
            .unwrap_or_default()
    }

    fn set_wave_source(&self, wave_source: Option<surfer_translation_types::WaveSource>) {
        let mut plugin = self.plugin.lock().unwrap();
        if plugin.function_exists("set_wave_source") {
            plugin
                .call::<_, ()>("set_wave_source", wave_source)
                .map_err(|e| {
                    error!(
                        "Failed to set_wave_source on {}. {e}",
                        self.file.to_string_lossy()
                    )
                })
                .ok();
        }
    }

    fn translate(
        &self,
        variable: &VariableMeta<VarId, ScopeId>,
        value: &VariableValue,
    ) -> eyre::Result<TranslationResult> {
        let result = self
            .plugin
            .lock()
            .unwrap()
            .call(
                "translate",
                TranslateParams {
                    variable: variable.clone().map_ids(|_| (), |_| ()),
                    value: value.clone(),
                },
            )
            .map_err(|e| {
                anyhow!(
                    "Failed to translate {} with {}. {e}",
                    variable.var.name,
                    self.file.to_string_lossy()
                )
            })?;
        Ok(result)
    }

    fn variable_info(&self, variable: &VariableMeta<VarId, ScopeId>) -> eyre::Result<VariableInfo> {
        let result = self
            .plugin
            .lock()
            .unwrap()
            .call("variable_info", variable.clone().map_ids(|_| (), |_| ()))
            .map_err(|e| {
                anyhow!(
                    "Failed to get variable info for {} with {}. {e}",
                    variable.var.name,
                    self.file.to_string_lossy()
                )
            })?;
        Ok(result)
    }

    fn translates(
        &self,
        variable: &VariableMeta<VarId, ScopeId>,
    ) -> eyre::Result<TranslationPreference> {
        match self
            .plugin
            .lock()
            .unwrap()
            .call("translates", variable.clone().map_ids(|_| (), |_| ()))
        {
            Ok(r) => Ok(r),
            Err(e) => Err(anyhow!(e)),
        }
    }

    fn reload(&self, _sender: std::sync::mpsc::Sender<Message>) {
        let mut plugin = self.plugin.lock().unwrap();
        if plugin.function_exists("reload") {
            match plugin.call("reload", ()) {
                Ok(()) => (),
                Err(e) => error!("{e:#}"),
            }
        }
    }

    fn variable_name_info(
        &self,
        variable: &VariableMeta<VarId, ScopeId>,
    ) -> Option<VariableNameInfo> {
        let mut plugin = self.plugin.lock().unwrap();
        if plugin.function_exists("variable_name_info") {
            match plugin.call(
                "variable_name_info",
                variable.clone().map_ids(|_| (), |_| ()),
            ) {
                Ok(result) => result,
                Err(e) => {
                    error!("{e:#}");
                    None
                }
            }
        } else {
            None
        }
    }
}

host_fn!(current_dir() -> String {
    std::env::current_dir()
        .with_context(|| "Failed to get current dir".to_string())
        .and_then(|dir| {
            dir.to_str().ok_or_else(|| {
                anyhow!("{} is not valid utf8", dir.to_string_lossy())
            }).map(|s| s.to_string())
        })
        .map_err(|e| extism::Error::msg(format!("{e:#}")))
});

host_fn!(read_file(filename: String) -> Vec<u8> {
    std::fs::read(Utf8PathBuf::from(&filename))
        .with_context(|| format!("Failed to read {filename}"))
        .map_err(|e| extism::Error::msg(format!("{e:#}")))
});

host_fn!(file_exists(filename: String) -> bool {
    Ok(Utf8PathBuf::from(&filename).exists())
});
