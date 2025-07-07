use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
};

use base64::{prelude::BASE64_STANDARD, Engine as _};
use futures::executor::block_on;
use num::{bigint::ToBigInt as _, BigInt, BigUint};
use rayon::iter::{IntoParallelRefIterator as _, ParallelIterator as _};
use surfer_translation_types::VariableValue;
use tokio::sync::RwLock;

use crate::{
    cxxrtl_container::CxxrtlItem,
    message::Message,
    wave_container::{QueryResult, VariableRef},
    EGUI_CONTEXT,
};

use super::sc_message::CxxrtlSample;

type ValueList = Arc<RwLock<BTreeMap<BigInt, HashMap<VariableRef, VariableValue>>>>;

pub struct QueryContainer {
    variable_values: ValueList,
}

impl QueryContainer {
    pub fn empty() -> Self {
        QueryContainer {
            variable_values: Arc::new(RwLock::new(BTreeMap::new())),
        }
    }

    pub fn populate(
        &mut self,
        variables: Vec<VariableRef>,
        item_info: Arc<HashMap<VariableRef, CxxrtlItem>>,
        data: Vec<CxxrtlSample>,
        msg_sender: std::sync::mpsc::Sender<Message>,
    ) {
        let variable_values = self.variable_values.clone();

        let task = fill_variable_values(variables, item_info, data, variable_values, msg_sender);
        #[cfg(target_arch = "wasm32")]
        wasm_bindgen_futures::spawn_local(task);
        #[cfg(not(target_arch = "wasm32"))]
        tokio::task::spawn(task);
    }

    pub fn query(&self, var: &VariableRef, query_time: BigInt) -> QueryResult {
        let values = block_on(self.variable_values.read());

        if let Some((time, value_map)) = values.range(..query_time.clone()).next_back() {
            match (time.to_biguint(), value_map.get(var)) {
                (Some(time), Some(value)) => {
                    let next = values
                        .range(query_time..)
                        .next()
                        .and_then(|(k, _)| k.to_biguint());
                    QueryResult {
                        current: Some((time.clone(), value.clone())),
                        next,
                    }
                }
                _ => QueryResult::default(),
            }
        } else {
            QueryResult::default()
        }
    }
}

async fn fill_variable_values(
    variables: Vec<VariableRef>,
    item_info: Arc<HashMap<VariableRef, CxxrtlItem>>,
    data: Vec<CxxrtlSample>,
    variable_values: ValueList,
    msg_sender: std::sync::mpsc::Sender<Message>,
) {
    let work = move || {
        // Once we base64 decode the cxxrtl data, we'll end up with a bunch of u32s, where
        // the variables are packed next to each other. We'll start off computing the offset
        // of each variable for later use
        let mut offset = 0;
        let mut ranges = vec![];
        for variable in &variables {
            let this_size_bits = &item_info[variable].width;
            let this_size_u32 = 1 + ((this_size_bits - 1) / 32);
            ranges.push((offset * 4) as usize..((offset + this_size_u32) * 4) as usize);
            offset += this_size_u32;
        }

        data.par_iter().for_each(|sample| {
            let u8s = BASE64_STANDARD
                .decode(&sample.item_values)
                .map_err(|e| {
                    panic!(
                        "Got non-base64 data from cxxrtl at time {}. {e}",
                        sample.time
                    )
                })
                .unwrap();

            let values = ranges
                .iter()
                .zip(&variables)
                .map(|(range, var)| {
                    let value = BigUint::from_bytes_le(&u8s[range.clone()]);

                    // FIXME: Probably shouldn't have this indexed by the variable ref here so we can
                    // avoid the clone
                    (var.clone(), VariableValue::BigUint(value))
                })
                .collect::<HashMap<_, _>>();

            block_on(variable_values.write())
                .insert(sample.time.as_femtoseconds().to_bigint().unwrap(), values);
            msg_sender
                .send(Message::InvalidateDrawCommands)
                .expect("Message receiver disconnected");
        });

        if let Some(ctx) = EGUI_CONTEXT.read().unwrap().as_ref() {
            ctx.request_repaint();
        }
    };
    // Since this is a purely CPU bound operation, we'll spawn a blocking task to
    // perform it. We can't do this on wasm though, so there we'll just run it normally
    // for now
    #[cfg(target_arch = "wasm32")]
    work();
    #[cfg(not(target_arch = "wasm32"))]
    tokio::task::spawn_blocking(work);
}
