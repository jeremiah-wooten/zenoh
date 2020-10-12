//
// Copyright (c) 2017, 2020 ADLINK Technology Inc.
//
// This program and the accompanying materials are made available under the
// terms of the Eclipse Public License 2.0 which is available at
// http://www.eclipse.org/legal/epl-2.0, or the Apache License, Version 2.0
// which is available at https://www.apache.org/licenses/LICENSE-2.0.
//
// SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
//
// Contributors:
//   ADLINK zenoh team, <zenoh@adlink-labs.tech>
//

//! The network level zenoh API.
//!
//! # Examples
//!
//! ### Publish
//! ```
//! use zenoh::net::*;
//!
//! #[async_std::main]
//! async fn main() {
//!     let session = open(config::default()).await.unwrap();
//!     session.write(&"/resource/name".into(), "value".as_bytes().into()).await.unwrap();
//!     session.close().await.unwrap();
//! }
//! ```
//!
//! ### Subscribe
//! ```no_run
//! use zenoh::net::*;
//! use futures::prelude::*;
//!
//! #[async_std::main]
//! async fn main() {
//!     let session = open(config::default()).await.unwrap();
//!     let sub_info = SubInfo {
//!         reliability: Reliability::Reliable,
//!         mode: SubMode::Push,
//!         period: None
//!     };
//!     let mut subscriber = session.declare_subscriber(&"/resource/name".into(), &sub_info).await.unwrap();
//!     while let Some(sample) = subscriber.stream().next().await { println!("Received : {:?}", sample); };
//! }
//! ```
//!
//! ### Query
//! ```
//! use zenoh::net::*;
//! use futures::prelude::*;
//!
//! #[async_std::main]
//! async fn main() {
//!     let session = open(config::default()).await.unwrap();
//!     let mut replies = session.query(
//!         &"/resource/name".into(),
//!         "predicate",
//!         QueryTarget::default(),
//!         QueryConsolidation::default()
//!     ).await.unwrap();
//!     while let Some(reply) = replies.next().await {
//!         println!(">> Received {:?}", reply.data);
//!     }
//! }
//! ```

use async_std::sync::channel;
use futures::prelude::*;
use log::{debug, trace};
use zenoh_protocol::core::WhatAmI;
use zenoh_router::runtime::config::*;
use zenoh_router::runtime::orchestrator::{Loop, SessionOrchestrator};
use zenoh_router::runtime::prelude::*;
mod types;
use git_version::git_version;
pub use types::*;

pub mod info;

#[macro_use]
mod session;
pub use session::*;

pub use zenoh_protocol::proto::{data_kind, encoding};

pub mod queryable {
    pub use zenoh_protocol::core::queryable::*;
}

pub use zenoh_router::runtime::config;

pub mod utils {
    pub mod resource_name {
        pub use zenoh_protocol::core::rname::intersect;
    }
}

const GIT_VERSION: &str = git_version!(prefix = "v");

/// Scout for routers and/or peers.
///
/// [scout](scout) spawns a task that periodically sends scout messages and returns
/// a [HelloStream](HelloStream) : a stream of received [Hello](Hello) messages.
///
/// Drop the returned [HelloStream](HelloStream) to stop the scouting task.
///
/// # Arguments
///
/// * `what` - The kind of zenoh process to scout for
/// * `config` - The configuration [Properties](Properties) to use for scouting
///
/// # Examples
/// ```no_run
/// # async_std::task::block_on(async {
/// use zenoh::net::*;
/// use futures::prelude::*;
///
/// let mut stream = scout(whatami::PEER | whatami::ROUTER, config::default()).await;
/// while let Some(hello) = stream.next().await {
///     println!("{}", hello);
/// }
/// # })
/// ```
pub async fn scout(what: WhatAmI, config: Properties) -> HelloStream {
    debug!("scout({}, {})", what, config::to_string(&config));
    let addr = config
        .last_or_str(ZN_MULTICAST_ADDRESS_KEY, ZN_MULTICAST_ADDRESS_DEFAULT)
        .parse()
        .unwrap();
    let iface = config.last_or_str(ZN_MULTICAST_INTERFACE_KEY, ZN_MULTICAST_INTERFACE_DEFAULT);

    let (hello_sender, hello_receiver) = channel::<Hello>(1);
    let (stop_sender, mut stop_receiver) = channel::<()>(1);
    let iface = SessionOrchestrator::get_interface(iface).unwrap();
    let socket = SessionOrchestrator::bind_ucast_port(iface).await.unwrap();
    async_std::task::spawn(async move {
        let hello_sender = &hello_sender;
        let scout = SessionOrchestrator::scout(&socket, what, &addr, async move |hello| {
            hello_sender.send(hello).await;
            Loop::Continue
        });
        let stop = async move {
            stop_receiver.next().await;
            trace!("stop scout({}, {})", what, iface);
        };
        async_std::prelude::FutureExt::race(scout, stop).await;
    });

    HelloStream {
        hello_receiver,
        stop_sender,
    }
}

/// Open a zenoh-net [Session](Session).
///
/// # Arguments
///
/// * `config` - The configuration [Properties](Properties) for the zenoh-net session
///
/// # Examples
/// ```
/// # async_std::task::block_on(async {
/// use zenoh::net::*;
///
/// let session = open(config::peer()).await.unwrap();
/// # })
/// ```
///
/// # Configuration Properties
///
/// [Properties](Properties) are a list of key/value pairs where key is a
/// [ZInt](ZInt) and value is a `Vec<u8>`.
/// Constants for the accepted keys can be found in the [config](config) module.
/// Multiple definition of the same key is allowed. If only one value is expected,
/// the last occurence is used.
///
/// # Examples
/// ```
/// # async_std::task::block_on(async {
/// use zenoh::net::*;
///
/// let mut config = config::peer();
/// config.push((config::ZN_LOCAL_ROUTING_KEY, b"false".to_vec()));
/// config.push((config::ZN_PEER_KEY, b"tcp/10.10.10.10:7447".to_vec()));
/// config.push((config::ZN_PEER_KEY, b"tcp/11.11.11.11:7447".to_vec()));
///
/// let session = open(config).await.unwrap();
/// # })
/// ```
pub async fn open(config: Properties) -> ZResult<Session> {
    debug!("Zenoh Rust API {}", GIT_VERSION);
    debug!("open({})", config::to_string(&config));
    Session::new(config).await
}