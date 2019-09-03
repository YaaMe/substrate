// Copyright 2018-2019 Parity Technologies (UK) Ltd.
// This file is part of Substrate.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Substrate.  If not, see <http://www.gnu.org/licenses/>.

#![warn(unused_extern_crates)]

//! Service and ServiceFactory implementation. Specialized wrapper over substrate service.

use substrate_service::{
	AbstractService, ServiceBuilder, config::Configuration, error::{Error as ServiceError},
};


use std::sync::Arc;
use std::time::Duration;
use std::path::PathBuf;
use futures03::compat;
use futures03::future::TryFutureExt;
use futures03::compat::Future01CompatExt;
use futures03::future::FutureExt;
use badger::{badger_import_queue,  BadgerImportQueue,Config as BadgerConfig,run_honey_badger};
use badger_primitives::{SignatureWrap};
use client::{self, LongestChain};
use badger::{self,};// FinalityProofProvider as BadgerFinalityProofProvider,};
use badger_primitives::{PublicKeyShareWrap,SecretKeyShareWrap,SecretKeyWrap, PublicKeyWrap, PublicKeySetWrap};
use hb_node_executor;
use primitives::Pair;
use futures::prelude::*;
use hb_node_primitives::{Block};
use hb_node_runtime::{GenesisConfig, RuntimeApi};

use transaction_pool::{self, txpool::{Pool as TransactionPool}};
use inherents::InherentDataProviders;
use network::construct_simple_protocol;
use network::{config::DummyFinalityProofRequestBuilder};

//use substrate_service::construct_service_factory;
use log::info;
use parking_lot::Mutex;

construct_simple_protocol! {
	/// Demo protocol attachment for substrate.
	pub struct NodeProtocol where Block = Block { }
}

/// Node specific configuration
pub struct NodeConfig {
	/// grandpa connection to import block
	// FIXME #1134 rather than putting this on the config, let's have an actual intermediate setup state
	pub inherent_data_providers: InherentDataProviders,
	num_validators: usize,
	secret_key_share: Option<SecretKeyShareWrap>,
	//node_key: SecretKeyWrap,
//	public_key_set: PublicKeySetWrap,

}

impl Default for NodeConfig {
	fn default() -> NodeConfig {
		NodeConfig {
			inherent_data_providers: InherentDataProviders::new(),
			num_validators:4,
			secret_key_share:None,
		}
	}
}


/// Starts a `ServiceBuilder` for a full service.
///
/// Use this macro if you don't actually need the full service, but just the builder in order to
/// be able to perform chain operations.
/// 
	
macro_rules! new_full_start {
	($config:expr) => {{
	
		let inherent_data_providers = inherents::InherentDataProviders::new();


		let builder = substrate_service::ServiceBuilder::new_full::<
			hb_node_primitives::Block, hb_node_runtime::RuntimeApi, hb_node_executor::Executor
		>($config)?
			.with_select_chain(|_config, backend| {
				#[allow(deprecated)]
				Ok(client::LongestChain::new(backend.clone()))
			})?
			.with_transaction_pool(|config, client|
				Ok(transaction_pool::txpool::Pool::new(config, transaction_pool::ChainApi::new(client)))
			)?
			.with_import_queue(|config, client, mut select_chain, transaction_pool| {
				let select_chain = select_chain.take()
					.ok_or_else(|| substrate_service::Error::SelectChainRequired)?;

				 #[allow(deprecated)]
				let fprb = Box::new(DummyFinalityProofRequestBuilder::default()) as Box<_>;
   			    let block_import=client.clone();
				/*let (block_import, link_half) =
					grandpa::block_import::<_, _, _, RuntimeApi, FullClient<Self>, _>(
						client.clone(), client.clone(), select_chain
					)?;*/
				//let justification_import = block_import.clone();
				badger_import_queue::<_, _, PublicKeyWrap,SignatureWrap>(
					Box::new(block_import),
					None,
					None,
					client,
					InherentDataProviders::new(),
				).map_err(Into::into)
							
			})?
			.with_rpc_extensions(|client, pool| {
				use hb_node_rpc::accounts::{Accounts, AccountsApi};

				let mut io = jsonrpc_core::IoHandler::<substrate_service::RpcMetadata>::default();
				io.extend_with(
					AccountsApi::to_delegate(Accounts::new(client, pool))
				);
				io
			})?;

		(builder,  inherent_data_providers, )
	}}
}

/// Creates a full service from the configuration.
///
/// We need to use a macro because the test suit doesn't work with an opaque service. It expects
/// concrete types instead.
macro_rules! new_full {
	($config:expr) => {{
		use futures::Future;
        let nconf_name=$config.n_conf_file.clone();
		let node_name=$config.name.clone();
		
		let (builder,  inherent_data_providers,) = new_full_start!($config);

		let service = builder.with_network_protocol(|_| Ok(crate::service::NodeProtocol::new()))?
			.with_opt_finality_proof_provider(|_client,_|
				Ok(None)
			)?
			.build()?;

	
		// spawn any futures that were created in the previous setup steps
	/*	for task in tasks_to_spawn.drain(..) {
			service.spawn_task(
				task.select(service.on_exit())
					.map(|_| ())
					.map_err(|_| ())
			);
		}*/

		let client = service.client().clone();
		let t_pool = service.transaction_pool();
		let select_chain = service.select_chain().ok_or(ServiceError::SelectChainRequired)?;
		let nconf= match &nconf_name	
			{
			Some(name) => PathBuf::from(name),
			None => PathBuf::from("./nodes.json")
			};
		let badger = run_honey_badger(
								client,
								t_pool,
								BadgerConfig::from_json_file_with_name(nconf,&node_name).unwrap(),
								service.network(),
								service.on_exit().clone().compat().map(|_| ()),
								Arc::new(Mutex::new(service.client().clone())), //block_import?
								//service.config().custom.inherent_data_providers.clone(),
								InherentDataProviders::new(),
								select_chain,
							)?;
		service.spawn_task(badger.unit_error()
			.boxed()
			.compat());

		Ok((service, inherent_data_providers))
	}}
}

/// Builds a new service for a full client.
pub fn new_full<C: Send + Default + 'static>(config: Configuration<C, GenesisConfig>)
-> Result<impl AbstractService, ServiceError> {
	new_full!(config).map(|(service, _)| service)
}

/// Builds a new service for a light client.
pub fn new_light<C: Send + Default + 'static>(config: Configuration<C, GenesisConfig>)
-> Result<impl AbstractService, ServiceError> {
 new_full!(config).map(|(service, _)| service)
}
