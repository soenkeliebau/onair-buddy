use pipewire::prelude::ReadableDict;
use pipewire::spa::{ForeignDict, ParsableValue};
use pipewire::types::ObjectType;
use pipewire::{Context, MainLoop};
use snafu::prelude::*;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};
use tracing::{info, debug, warn};

#[derive(Debug, Snafu)]
pub enum Error {
    #[snafu(display("No output node id present in properties"))]
    NoOutputNode { props: String },
    #[snafu(display("No input node id present in properties"))]
    NoInputNode { props: String },
}

#[derive(Default)]
pub struct State {
    headset_id: Option<u32>,
    active_links: HashSet<u32>,
}

impl State {
    pub fn is_link_in_scope(&self, output_node: &u32) -> bool {
        self.headset_id.map_or(false, |id| id.eq(output_node))
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt().init();
    let local_registry: Arc<RwLock<HashMap<u32, String>>> = Arc::new(RwLock::new(HashMap::new()));
    let state: Arc<RwLock<State>> = Arc::new(RwLock::new(State::default()));
    let global_state = state.clone();
    let remove_state = state.clone();

    let mainloop = MainLoop::new()?;
    let context = Context::new(&mainloop)?;
    let core = context.connect(None)?;
    let registry = core.get_registry()?;

    let _listener = registry
        .add_listener_local()
        .global(move |global| {
            match global.type_ {
                ObjectType::Node => {
                    if let Some(node_props) = &global.props {
                        if let Some(node_description) = node_props.get("node.description") {
                            if node_description.eq("Jabra Engage 75 Mono") {
                                info!("Identified id [{}] as headset", global.id);
                                let state_local = global_state.clone();
                                let mut state_write = state_local.write().unwrap();
                                state_write.headset_id = Some(global.id);
                                // Need to decide if we need to reset active links here somehow
                                drop(state_write);
                            }
                            local_registry
                                .clone()
                                .write()
                                .unwrap()
                                .insert(global.id, node_description.to_string());
                        };
                    }
                    debug!("done with node [{}]", global.id);
                }

                ObjectType::Link => {
                    let reg = local_registry.clone();
                    if let Some(link_props) = &global.props {
                        let local_state = global_state.clone();
                        let state_read = local_state.read().unwrap();
                        let input_node =
                            u32::parse_value(get_input_node(&link_props).unwrap()).unwrap();
                        let output_node =
                            u32::parse_value(get_output_node(&link_props).unwrap()).unwrap();
                        if state_read.is_link_in_scope(&output_node) {
                            // Need to drop read here, otherwise no writy below
                            drop(state_read);
                            info!("found in scope link [{}] from [{}] to [{}]", global.id, output_node, input_node);
                            global_state
                                .clone()
                                .write()
                                .unwrap()
                                .active_links
                                .insert(global.id);
                            info!("On Air: [{:?}]", check_if_on_air(global_state.clone()));
                            debug!("dropped write lock");
                        } else {
                            let reg_read = reg.read().unwrap();
                            debug!(
                                "New Link: [{:?}] from [{}] to [{}]",
                                global.id,
                                reg_read
                                    .get(&output_node)
                                    .unwrap_or(&"undefined".to_string()),
                                reg_read
                                    .get(&input_node)
                                    .unwrap_or(&&"undefined".to_string())
                            );
                        }
                    }
                    debug!("done with link [{}]", global.id);
                }
                _ => {
                    // Other objects are not interesting to us
                }
            };
        })
        .global_remove(move |id| {
            if remove_state.clone().read().unwrap().active_links.contains(&id) {
                info!("In scope link [{}] removed.", id);
                remove_state.clone().write().unwrap().active_links.remove(&id);
                info!("On Air: [{:?}]", check_if_on_air(remove_state.clone()));

            }
        })
        .register();




    // Calling the `destroy_global` method on the registry will destroy the object with the specified id on the remote.
    // We don't have a specific object to destroy now, so this is commented out.
    // registry.destroy_global(313).into_result()?;

    mainloop.run();

    Ok(())
}

pub fn get_input_node(props: &ForeignDict) -> Result<&str, Error> {
    props.get("link.input.node").context(NoInputNodeSnafu {
        props: format!("{:?}", props),
    })
}

pub fn get_output_node(props: &ForeignDict) -> Result<&str, Error> {
    props.get("link.output.node").context(NoOutputNodeSnafu {
        props: format!("{:?}", props),
    })
}

pub fn check_if_on_air(state: Arc<RwLock<State>>) -> bool {
    !state.read().unwrap().active_links.is_empty()
}
