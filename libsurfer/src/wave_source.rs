use std::fmt::{Display, Formatter};
use std::fs;
use std::io::Cursor;
use std::sync::atomic::AtomicU64;
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::sync::Mutex;

use crate::async_util::{perform_async_work, perform_work, sleep_ms};
use crate::cxxrtl_container::CxxrtlContainer;
use crate::spawn;
use crate::util::get_multi_extension;
use camino::{Utf8Path, Utf8PathBuf};
use eyre::Result;
use eyre::{anyhow, WrapErr};
use ftr_parser::parse;
use futures_util::FutureExt;
use log::{error, info, warn};
use serde::{Deserialize, Serialize};
use web_time::Instant;

use crate::transaction_container::TransactionContainer;
use crate::wave_container::WaveContainer;
use crate::wellen::{
    BodyResult, HeaderResult, LoadSignalPayload, LoadSignalsCmd, LoadSignalsResult,
};
use crate::{message::Message, SystemState};
use surver::{Status, HTTP_SERVER_KEY, HTTP_SERVER_VALUE_SURFER, WELLEN_SURFER_DEFAULT_OPTIONS};

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub enum CxxrtlKind {
    Tcp { url: String },
    Mailbox,
}
impl std::fmt::Display for CxxrtlKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            CxxrtlKind::Tcp { url } => write!(f, "cxxrtl+tcp://{url}"),
            CxxrtlKind::Mailbox => write!(f, "cxxrtl mailbox"),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub enum WaveSource {
    File(Utf8PathBuf),
    Data,
    DragAndDrop(Option<Utf8PathBuf>),
    Url(String),
    Cxxrtl(CxxrtlKind),
}

pub const STATE_FILE_EXTENSION: &str = "surf.ron";

impl WaveSource {
    pub fn as_file(&self) -> Option<&Utf8Path> {
        match self {
            WaveSource::File(path) => Some(path.as_path()),
            _ => None,
        }
    }

    pub fn path(&self) -> Option<&Utf8PathBuf> {
        match self {
            WaveSource::File(path) => Some(path),
            WaveSource::DragAndDrop(Some(path)) => Some(path),
            _ => None,
        }
    }

    pub fn sibling_state_file(&self) -> Option<Utf8PathBuf> {
        let path = self.path()?;
        let directory = path.parent()?;
        let paths = fs::read_dir(directory).ok()?;

        for entry in paths {
            let Ok(entry) = entry else { continue };
            if let Ok(path) = Utf8PathBuf::from_path_buf(entry.path()) {
                let Some(ext) = get_multi_extension(&path) else {
                    continue;
                };
                if ext.as_str() == STATE_FILE_EXTENSION {
                    return Some(path);
                }
            }
        }

        None
    }

    pub fn into_translation_type(&self) -> surfer_translation_types::WaveSource {
        use surfer_translation_types::WaveSource as Ws;
        match self {
            WaveSource::File(file) => Ws::File(file.to_string()),
            WaveSource::Data => Ws::Data,
            WaveSource::DragAndDrop(file) => Ws::DragAndDrop(file.as_ref().map(|f| f.to_string())),
            WaveSource::Url(u) => Ws::Url(u.clone()),
            WaveSource::Cxxrtl(_) => Ws::Cxxrtl,
        }
    }
}

pub fn url_to_wavesource(url: &str) -> Option<WaveSource> {
    if url.starts_with("https://") || url.starts_with("http://") {
        info!("Wave source is url");
        Some(WaveSource::Url(url.to_string()))
    } else if url.starts_with("cxxrtl+tcp://") {
        #[cfg(not(target_arch = "wasm32"))]
        {
            info!("Wave source is cxxrtl tcp");
            Some(WaveSource::Cxxrtl(CxxrtlKind::Tcp {
                url: url.replace("cxxrtl+tcp://", ""),
            }))
        }
        #[cfg(target_arch = "wasm32")]
        {
            log::warn!("Loading waves from cxxrtl via tcp is unsupported in WASM builds.");
            None
        }
    } else {
        None
    }
}

pub fn string_to_wavesource(path: &str) -> WaveSource {
    if let Some(source) = url_to_wavesource(path) {
        source
    } else {
        info!("Wave source is file");
        WaveSource::File(path.into())
    }
}

impl Display for WaveSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WaveSource::File(file) => write!(f, "{file}"),
            WaveSource::Data => write!(f, "File data"),
            WaveSource::DragAndDrop(None) => write!(f, "Dropped file"),
            WaveSource::DragAndDrop(Some(filename)) => write!(f, "Dropped file ({filename})"),
            WaveSource::Url(url) => write!(f, "{url}"),
            WaveSource::Cxxrtl(CxxrtlKind::Tcp { url }) => write!(f, "cxxrtl+tcp://{url}"),
            WaveSource::Cxxrtl(CxxrtlKind::Mailbox) => write!(f, "cxxrtl mailbox"),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Copy, Clone, Serialize, Deserialize)]
pub enum WaveFormat {
    Vcd,
    Fst,
    Ghw,
    CxxRtl,
    Ftr,
}

impl Display for WaveFormat {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            WaveFormat::Vcd => write!(f, "VCD"),
            WaveFormat::Fst => write!(f, "FST"),
            WaveFormat::Ghw => write!(f, "GHW"),
            WaveFormat::CxxRtl => write!(f, "Cxxrtl"),
            WaveFormat::Ftr => write!(f, "FTR"),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct LoadOptions {
    pub keep_variables: bool,
    pub keep_unavailable: bool,
}

impl LoadOptions {
    pub fn clean() -> Self {
        Self {
            keep_variables: false,
            keep_unavailable: false,
        }
    }
}

pub struct LoadProgress {
    pub started: Instant,
    pub progress: LoadProgressStatus,
}

impl LoadProgress {
    pub fn new(progress: LoadProgressStatus) -> Self {
        LoadProgress {
            started: Instant::now(),
            progress,
        }
    }
}

pub enum LoadProgressStatus {
    Downloading(String),
    Connecting(String),
    ReadingHeader(WaveSource),
    ReadingBody(WaveSource, u64, Arc<AtomicU64>),
    LoadingVariables(u64),
}

impl SystemState {
    pub fn load_from_file(
        &mut self,
        filename: Utf8PathBuf,
        load_options: LoadOptions,
    ) -> Result<()> {
        match get_multi_extension(&filename) {
            Some(ext) => match ext.as_str() {
                STATE_FILE_EXTENSION => {
                    self.load_state_file(Some(filename.into_std_path_buf()));
                    Ok(())
                }
                "ftr" => self.load_transactions_from_file(filename, load_options),
                _ => self.load_wave_from_file(filename, load_options),
            },
            _ => self.load_wave_from_file(filename, load_options),
        }
    }

    pub fn load_from_bytes(
        &mut self,
        source: WaveSource,
        bytes: Vec<u8>,
        load_options: LoadOptions,
    ) {
        if parse::is_ftr(&mut Cursor::new(&bytes)) {
            self.load_transactions_from_bytes(source, bytes, load_options);
        } else {
            self.load_wave_from_bytes(source, bytes, load_options);
        }
    }

    pub fn load_wave_from_file(
        &mut self,
        filename: Utf8PathBuf,
        load_options: LoadOptions,
    ) -> Result<()> {
        info!("Loading a waveform file: {filename}");
        let start = web_time::Instant::now();
        let source = WaveSource::File(filename.clone());
        let source_copy = source.clone();
        let sender = self.channels.msg_sender.clone();

        perform_work(move || {
            let header_result = wellen::viewers::read_header_from_file(
                filename.as_str(),
                &WELLEN_SURFER_DEFAULT_OPTIONS,
            )
            .map_err(|e| anyhow!("{e:?}"))
            .with_context(|| format!("Failed to parse wave file: {source}"));

            match header_result {
                Ok(header) => {
                    let msg = Message::WaveHeaderLoaded(
                        start,
                        source,
                        load_options,
                        HeaderResult::LocalFile(Box::new(header)),
                    );
                    sender.send(msg).unwrap();
                }
                Err(e) => sender.send(Message::Error(e)).unwrap(),
            }
        });

        self.progress_tracker = Some(LoadProgress::new(LoadProgressStatus::ReadingHeader(
            source_copy,
        )));
        Ok(())
    }

    pub fn load_from_data(&mut self, data: Vec<u8>, load_options: LoadOptions) -> Result<()> {
        self.load_from_bytes(WaveSource::Data, data, load_options);
        Ok(())
    }

    pub fn load_from_dropped(&mut self, file: egui::DroppedFile) -> Result<()> {
        info!("Got a dropped file");

        let path = file.path.and_then(|x| Utf8PathBuf::try_from(x).ok());

        if let Some(bytes) = file.bytes {
            if bytes.is_empty() {
                Err(anyhow!("Dropped an empty file"))
            } else {
                if let Some(path) = path.clone() {
                    if get_multi_extension(&path) == Some(STATE_FILE_EXTENSION.to_string()) {
                        let sender = self.channels.msg_sender.clone();
                        perform_async_work(async move {
                            let new_state = match ron::de::from_bytes(&bytes)
                                .context(format!("Failed loading {}", path))
                            {
                                Ok(s) => s,
                                Err(e) => {
                                    error!("Failed to load state: {e:#?}");
                                    return;
                                }
                            };

                            sender
                                .send(Message::LoadState(
                                    new_state,
                                    Some(path.into_std_path_buf()),
                                ))
                                .unwrap();
                        });
                    } else {
                        self.load_from_bytes(
                            WaveSource::DragAndDrop(Some(path)),
                            bytes.to_vec(),
                            LoadOptions::clean(),
                        );
                    }
                } else {
                    self.load_from_bytes(
                        WaveSource::DragAndDrop(path),
                        bytes.to_vec(),
                        LoadOptions::clean(),
                    );
                }
                Ok(())
            }
        } else if let Some(path) = path {
            self.load_from_file(path, LoadOptions::clean())
        } else {
            Err(anyhow!(
                "Unknown how to load dropped file w/o path or bytes"
            ))
        }
    }

    pub fn load_wave_from_url(&mut self, url: String, load_options: LoadOptions) {
        match url_to_wavesource(&url) {
            // We want to support opening cxxrtl urls using open url and friends,
            // so we'll special case
            #[cfg(not(target_arch = "wasm32"))]
            Some(WaveSource::Cxxrtl(kind)) => {
                self.connect_to_cxxrtl(kind, load_options.keep_variables);
            }
            // However, if we don't get a cxxrtl url, we want to continue loading this as
            // a url even if it isn't auto detected as a url.
            _ => {
                let sender = self.channels.msg_sender.clone();
                let url_ = url.clone();
                let task = async move {
                    let maybe_response = reqwest::get(&url)
                        .map(|e| e.with_context(|| format!("Failed fetch download {url}")))
                        .await;
                    let response: reqwest::Response = match maybe_response {
                        Ok(r) => r,
                        Err(e) => {
                            sender.send(Message::Error(e)).unwrap();
                            return;
                        }
                    };

                    // check to see if the response came from a Surfer running in server mode
                    if let Some(value) = response.headers().get(HTTP_SERVER_KEY) {
                        if matches!(value.to_str(), Ok(HTTP_SERVER_VALUE_SURFER)) {
                            info!("Connecting to a surfer server at: {url}");
                            // request status and hierarchy
                            Self::get_server_status(sender.clone(), url.clone(), 0);
                            Self::get_hierarchy_from_server(
                                sender.clone(),
                                url.clone(),
                                load_options,
                            );
                            return;
                        }
                    }

                    // otherwise we load the body to get at the file
                    let bytes = response
                        .bytes()
                        .map(|e| e.with_context(|| format!("Failed to download {url}")))
                        .await;

                    match bytes {
                        Ok(b) => sender.send(Message::FileDownloaded(url, b, load_options)),
                        Err(e) => sender.send(Message::Error(e)),
                    }
                    .unwrap();
                };
                spawn!(task);

                self.progress_tracker =
                    Some(LoadProgress::new(LoadProgressStatus::Downloading(url_)));
            }
        }
    }

    pub fn load_transactions_from_file(
        &mut self,
        filename: camino::Utf8PathBuf,
        load_options: LoadOptions,
    ) -> Result<()> {
        info!("Loading a transaction file: {filename}");
        let sender = self.channels.msg_sender.clone();
        let source = WaveSource::File(filename.clone());
        let format = WaveFormat::Ftr;

        let result = ftr_parser::parse::parse_ftr(filename.into_std_path_buf());

        info!("Done with loading ftr file");

        match result {
            Ok(ftr) => sender
                .send(Message::TransactionStreamsLoaded(
                    source,
                    format,
                    TransactionContainer { inner: ftr },
                    load_options,
                ))
                .unwrap(),
            Err(e) => sender.send(Message::Error(e)).unwrap(),
        }
        Ok(())
    }
    pub fn load_transactions_from_bytes(
        &mut self,
        source: WaveSource,
        bytes: Vec<u8>,
        load_options: LoadOptions,
    ) {
        let sender = self.channels.msg_sender.clone();

        let result = parse::parse_ftr_from_bytes(bytes);

        info!("Done with loading ftr file");

        match result {
            Ok(ftr) => sender
                .send(Message::TransactionStreamsLoaded(
                    source,
                    WaveFormat::Ftr,
                    TransactionContainer { inner: ftr },
                    load_options,
                ))
                .unwrap(),
            Err(e) => sender.send(Message::Error(e)).unwrap(),
        }
    }
    fn get_hierarchy_from_server(
        sender: Sender<Message>,
        server: String,
        load_options: LoadOptions,
    ) {
        let start = web_time::Instant::now();
        let source = WaveSource::Url(server.clone());

        let task = async move {
            let res = crate::remote::get_hierarchy(server.clone())
                .await
                .map_err(|e| anyhow!("{e:?}"))
                .with_context(|| {
                    format!("Failed to retrieve hierarchy from remote server {server}")
                });

            match res {
                Ok(h) => {
                    let header = HeaderResult::Remote(Arc::new(h.hierarchy), h.file_format, server);
                    let msg = Message::WaveHeaderLoaded(start, source, load_options, header);
                    sender.send(msg).unwrap();
                }
                Err(e) => sender.send(Message::Error(e)).unwrap(),
            }
        };
        spawn!(task);
    }

    pub fn get_time_table_from_server(sender: Sender<Message>, server: String) {
        let start = web_time::Instant::now();
        let source = WaveSource::Url(server.clone());

        let task = async move {
            let res = crate::remote::get_time_table(server.clone())
                .await
                .map_err(|e| anyhow!("{e:?}"))
                .with_context(|| {
                    format!("Failed to retrieve time table from remote server {server}")
                });

            match res {
                Ok(table) => {
                    let msg =
                        Message::WaveBodyLoaded(start, source, BodyResult::Remote(table, server));
                    sender.send(msg).unwrap();
                }
                Err(e) => sender.send(Message::Error(e)).unwrap(),
            }
        };
        spawn!(task);
    }

    fn get_server_status(sender: Sender<Message>, server: String, delay_ms: u64) {
        let start = web_time::Instant::now();
        let task = async move {
            sleep_ms(delay_ms).await;
            let res = crate::remote::get_status(server.clone())
                .await
                .map_err(|e| anyhow!("{e:?}"))
                .with_context(|| format!("Failed to retrieve status from remote server {server}"));

            match res {
                Ok(status) => {
                    let msg = Message::SurferServerStatus(start, server, status);
                    sender.send(msg).unwrap();
                }
                Err(e) => sender.send(Message::Error(e)).unwrap(),
            }
        };
        spawn!(task);
    }

    /// uses the server status in order to display a loading bar
    pub fn server_status_to_progress(&mut self, server: String, status: Status) {
        // once the body is loaded, we are no longer interested in the status
        let body_loaded = self
            .user
            .waves
            .as_ref()
            .is_some_and(|w| w.inner.body_loaded());
        if !body_loaded {
            // the progress tracker will be cleared once the hierarchy is returned from the server
            let source = WaveSource::Url(server.clone());
            let sender = self.channels.msg_sender.clone();
            self.progress_tracker = Some(LoadProgress::new(LoadProgressStatus::ReadingBody(
                source,
                status.bytes,
                Arc::new(AtomicU64::new(status.bytes_loaded)),
            )));
            // get another status update
            Self::get_server_status(sender, server, 250);
        }
    }

    pub fn connect_to_cxxrtl(&mut self, kind: CxxrtlKind, keep_variables: bool) {
        let sender = self.channels.msg_sender.clone();

        self.progress_tracker = Some(LoadProgress::new(LoadProgressStatus::Connecting(format!(
            "{kind}"
        ))));

        let task = async move {
            let container = match &kind {
                #[cfg(not(target_arch = "wasm32"))]
                CxxrtlKind::Tcp { url } => {
                    CxxrtlContainer::new_tcp(url, self.channels.msg_sender.clone()).await
                }
                #[cfg(target_arch = "wasm32")]
                CxxrtlKind::Tcp { .. } => {
                    error!("Cxxrtl tcp is not supported om wasm");
                    return;
                }
                #[cfg(not(target_arch = "wasm32"))]
                CxxrtlKind::Mailbox => {
                    error!("CXXRTL mailboxes are only supported on wasm for now");
                    return;
                }
                #[cfg(target_arch = "wasm32")]
                CxxrtlKind::Mailbox => CxxrtlContainer::new_wasm_mailbox(sender.clone()).await,
            };

            match container {
                Ok(c) => sender.send(Message::WavesLoaded(
                    WaveSource::Cxxrtl(kind),
                    WaveFormat::CxxRtl,
                    Box::new(WaveContainer::Cxxrtl(Box::new(Mutex::new(c)))),
                    LoadOptions {
                        keep_variables,
                        keep_unavailable: false,
                    },
                )),
                Err(e) => sender.send(Message::Error(e)),
            }
            .unwrap()
        };
        #[cfg(not(target_arch = "wasm32"))]
        futures::executor::block_on(task);
        #[cfg(target_arch = "wasm32")]
        wasm_bindgen_futures::spawn_local(task);
    }

    pub fn load_wave_from_bytes(
        &mut self,
        source: WaveSource,
        bytes: Vec<u8>,
        load_options: LoadOptions,
    ) {
        let start = web_time::Instant::now();
        let sender = self.channels.msg_sender.clone();
        let source_copy = source.clone();
        perform_work(move || {
            let header_result =
                wellen::viewers::read_header(Cursor::new(bytes), &WELLEN_SURFER_DEFAULT_OPTIONS)
                    .map_err(|e| anyhow!("{e:?}"))
                    .with_context(|| format!("Failed to parse wave file: {source}"));

            match header_result {
                Ok(header) => {
                    let msg = Message::WaveHeaderLoaded(
                        start,
                        source,
                        load_options,
                        HeaderResult::LocalBytes(Box::new(header)),
                    );
                    sender.send(msg).unwrap();
                }
                Err(e) => sender.send(Message::Error(e)).unwrap(),
            }
        });

        self.progress_tracker = Some(LoadProgress::new(LoadProgressStatus::ReadingHeader(
            source_copy,
        )));
    }

    fn get_thread_pool() -> Option<rayon::ThreadPool> {
        // try to create a new rayon thread pool so that we do not block drawing functionality
        // which might be blocked by the waveform reader using up all the threads in the global pool
        match rayon::ThreadPoolBuilder::new().build() {
            Ok(pool) => Some(pool),
            Err(e) => {
                // on wasm this will always fail
                warn!("failed to create thread pool: {e:?}");
                None
            }
        }
    }

    pub fn load_wave_body<R: std::io::BufRead + std::io::Seek + Sync + Send + 'static>(
        &mut self,
        source: WaveSource,
        cont: wellen::viewers::ReadBodyContinuation<R>,
        body_len: u64,
        hierarchy: Arc<wellen::Hierarchy>,
    ) {
        let start = web_time::Instant::now();
        let sender = self.channels.msg_sender.clone();
        let source_copy = source.clone();
        let progress = Arc::new(AtomicU64::new(0));
        let progress_copy = progress.clone();
        let pool = Self::get_thread_pool();

        perform_work(move || {
            let action = || {
                let p = Some(progress_copy);
                let body_result = wellen::viewers::read_body(cont, &hierarchy, p)
                    .map_err(|e| anyhow!("{e:?}"))
                    .with_context(|| format!("Failed to parse body of wave file: {source}"));

                match body_result {
                    Ok(body) => {
                        let msg = Message::WaveBodyLoaded(start, source, BodyResult::Local(body));
                        sender.send(msg).unwrap();
                    }
                    Err(e) => sender.send(Message::Error(e)).unwrap(),
                }
            };
            if let Some(pool) = pool {
                pool.install(action);
            } else {
                action();
            }
        });

        self.progress_tracker = Some(LoadProgress::new(LoadProgressStatus::ReadingBody(
            source_copy,
            body_len,
            progress,
        )));
    }

    pub fn load_variables(&mut self, cmd: LoadSignalsCmd) {
        let (signals, from_unique_id, payload) = cmd.destruct();
        if signals.is_empty() {
            return;
        }
        let num_signals = signals.len() as u64;
        let start = web_time::Instant::now();
        let sender = self.channels.msg_sender.clone();

        match payload {
            LoadSignalPayload::Local(mut source, hierarchy) => {
                let pool = Self::get_thread_pool();

                perform_work(move || {
                    let action = || {
                        let loaded = source.load_signals(&signals, &hierarchy, true);
                        let res = LoadSignalsResult::local(source, loaded, from_unique_id);
                        let msg = Message::SignalsLoaded(start, res);
                        sender.send(msg).unwrap();
                    };
                    if let Some(pool) = pool {
                        pool.install(action);
                    } else {
                        action();
                    }
                });
            }
            LoadSignalPayload::Remote(server) => {
                let task = async move {
                    let res = crate::remote::get_signals(server.clone(), &signals)
                        .await
                        .map_err(|e| anyhow!("{e:?}"))
                        .with_context(|| {
                            format!("Failed to retrieve signals from remote server {server}")
                        });

                    match res {
                        Ok(loaded) => {
                            let res = LoadSignalsResult::remote(server, loaded, from_unique_id);
                            let msg = Message::SignalsLoaded(start, res);
                            sender.send(msg).unwrap();
                        }
                        Err(e) => sender.send(Message::Error(e)).unwrap(),
                    }
                };
                spawn!(task);
            }
        }

        self.progress_tracker = Some(LoadProgress::new(LoadProgressStatus::LoadingVariables(
            num_signals,
        )));
    }
}

pub fn draw_progress_information(ui: &mut egui::Ui, progress_data: &LoadProgress) {
    match &progress_data.progress {
        LoadProgressStatus::Connecting(url) => {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.monospace(format!("Connecting {url}"));
            });
        }
        LoadProgressStatus::Downloading(url) => {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.monospace(format!("Downloading {url}"));
            });
        }
        LoadProgressStatus::ReadingHeader(source) => {
            ui.spinner();
            ui.monospace(format!("Loading variable names from {source}"));
        }
        LoadProgressStatus::ReadingBody(source, 0, _) => {
            ui.spinner();
            ui.monospace(format!("Loading variable change data from {source}"));
        }
        LoadProgressStatus::LoadingVariables(num) => {
            ui.spinner();
            ui.monospace(format!("Loading {num} variables"));
        }
        LoadProgressStatus::ReadingBody(source, total, bytes_done) => {
            let num_bytes = bytes_done.load(std::sync::atomic::Ordering::SeqCst);
            let progress = num_bytes as f32 / *total as f32;
            ui.monospace(format!(
                "Loading variable change data from {source}. {} / {}",
                bytesize::ByteSize::b(num_bytes),
                bytesize::ByteSize::b(*total),
            ));
            let progress_bar = egui::ProgressBar::new(progress)
                .show_percentage()
                .desired_width(300.);
            ui.add(progress_bar);
        }
    };
}
