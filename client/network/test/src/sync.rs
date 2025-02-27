// Copyright 2017-2019 Parity Technologies (UK) Ltd.
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

use sc_network::config::Roles;
use consensus::BlockOrigin;
use futures03::TryFutureExt as _;
use std::time::Duration;
use tokio::runtime::current_thread;
use super::*;

fn test_ancestor_search_when_common_is(n: usize) {
	let _ = ::env_logger::try_init();
	let mut runtime = current_thread::Runtime::new().unwrap();
	let mut net = TestNet::new(3);

	net.peer(0).push_blocks(n, false);
	net.peer(1).push_blocks(n, false);
	net.peer(2).push_blocks(n, false);

	net.peer(0).push_blocks(10, true);
	net.peer(1).push_blocks(100, false);
	net.peer(2).push_blocks(100, false);

	net.block_until_sync(&mut runtime);
	let peer1 = &net.peers()[1];
	assert!(net.peers()[0].blockchain_canon_equals(peer1));
}

#[test]
fn sync_peers_works() {
	let _ = ::env_logger::try_init();
	let mut runtime = current_thread::Runtime::new().unwrap();
	let mut net = TestNet::new(3);

	runtime.block_on(futures::future::poll_fn::<(), (), _>(|| -> Result<_, ()> {
		net.poll();
		for peer in 0..3 {
			if net.peer(peer).num_peers() != 2 {
				return Ok(Async::NotReady)
			}
		}
		Ok(Async::Ready(()))
	})).unwrap();
}

#[test]
fn sync_cycle_from_offline_to_syncing_to_offline() {
	let _ = ::env_logger::try_init();
	let mut runtime = current_thread::Runtime::new().unwrap();
	let mut net = TestNet::new(3);
	for peer in 0..3 {
		// Offline, and not major syncing.
		assert!(net.peer(peer).is_offline());
		assert!(!net.peer(peer).is_major_syncing());
	}

	// Generate blocks.
	net.peer(2).push_blocks(100, false);

	// Block until all nodes are online and nodes 0 and 1 and major syncing.
	runtime.block_on(futures::future::poll_fn::<(), (), _>(|| -> Result<_, ()> {
		net.poll();
		for peer in 0..3 {
			// Online
			if net.peer(peer).is_offline() {
				return Ok(Async::NotReady)
			}
			if peer < 2 {
				// Major syncing.
				if !net.peer(peer).is_major_syncing() {
					return Ok(Async::NotReady)
				}
			}
		}
		Ok(Async::Ready(()))
	})).unwrap();

	// Block until all nodes are done syncing.
	runtime.block_on(futures::future::poll_fn::<(), (), _>(|| -> Result<_, ()> {
		net.poll();
		for peer in 0..3 {
			if net.peer(peer).is_major_syncing() {
				return Ok(Async::NotReady)
			}
		}
		Ok(Async::Ready(()))
	})).unwrap();

	// Now drop nodes 1 and 2, and check that node 0 is offline.
	net.peers.remove(2);
	net.peers.remove(1);
	runtime.block_on(futures::future::poll_fn::<(), (), _>(|| -> Result<_, ()> {
		net.poll();
		if !net.peer(0).is_offline() {
			Ok(Async::NotReady)
		} else {
			Ok(Async::Ready(()))
		}
	})).unwrap();
}

#[test]
fn syncing_node_not_major_syncing_when_disconnected() {
	let _ = ::env_logger::try_init();
	let mut runtime = current_thread::Runtime::new().unwrap();
	let mut net = TestNet::new(3);

	// Generate blocks.
	net.peer(2).push_blocks(100, false);

	// Check that we're not major syncing when disconnected.
	assert!(!net.peer(1).is_major_syncing());

	// Check that we switch to major syncing.
	runtime.block_on(futures::future::poll_fn::<(), (), _>(|| -> Result<_, ()> {
		net.poll();
		if !net.peer(1).is_major_syncing() {
			Ok(Async::NotReady)
		} else {
			Ok(Async::Ready(()))
		}
	})).unwrap();

	// Destroy two nodes, and check that we switch to non-major syncing.
	net.peers.remove(2);
	net.peers.remove(0);
	runtime.block_on(futures::future::poll_fn::<(), (), _>(|| -> Result<_, ()> {
		net.poll();
		if net.peer(0).is_major_syncing() {
			Ok(Async::NotReady)
		} else {
			Ok(Async::Ready(()))
		}
	})).unwrap();
}

#[test]
fn sync_from_two_peers_works() {
	let _ = ::env_logger::try_init();
	let mut runtime = current_thread::Runtime::new().unwrap();
	let mut net = TestNet::new(3);
	net.peer(1).push_blocks(100, false);
	net.peer(2).push_blocks(100, false);
	net.block_until_sync(&mut runtime);
	let peer1 = &net.peers()[1];
	assert!(net.peers()[0].blockchain_canon_equals(peer1));
	assert!(!net.peer(0).is_major_syncing());
}

#[test]
fn sync_from_two_peers_with_ancestry_search_works() {
	let _ = ::env_logger::try_init();
	let mut runtime = current_thread::Runtime::new().unwrap();
	let mut net = TestNet::new(3);
	net.peer(0).push_blocks(10, true);
	net.peer(1).push_blocks(100, false);
	net.peer(2).push_blocks(100, false);
	net.block_until_sync(&mut runtime);
	let peer1 = &net.peers()[1];
	assert!(net.peers()[0].blockchain_canon_equals(peer1));
}

#[test]
fn ancestry_search_works_when_backoff_is_one() {
	let _ = ::env_logger::try_init();
	let mut runtime = current_thread::Runtime::new().unwrap();
	let mut net = TestNet::new(3);

	net.peer(0).push_blocks(1, false);
	net.peer(1).push_blocks(2, false);
	net.peer(2).push_blocks(2, false);

	net.block_until_sync(&mut runtime);
	let peer1 = &net.peers()[1];
	assert!(net.peers()[0].blockchain_canon_equals(peer1));
}

#[test]
fn ancestry_search_works_when_ancestor_is_genesis() {
	let _ = ::env_logger::try_init();
	let mut runtime = current_thread::Runtime::new().unwrap();
	let mut net = TestNet::new(3);

	net.peer(0).push_blocks(13, true);
	net.peer(1).push_blocks(100, false);
	net.peer(2).push_blocks(100, false);

	net.block_until_sync(&mut runtime);
	let peer1 = &net.peers()[1];
	assert!(net.peers()[0].blockchain_canon_equals(peer1));
}

#[test]
fn ancestry_search_works_when_common_is_one() {
	test_ancestor_search_when_common_is(1);
}

#[test]
fn ancestry_search_works_when_common_is_two() {
	test_ancestor_search_when_common_is(2);
}

#[test]
fn ancestry_search_works_when_common_is_hundred() {
	test_ancestor_search_when_common_is(100);
}

#[test]
fn sync_long_chain_works() {
	let _ = ::env_logger::try_init();
	let mut runtime = current_thread::Runtime::new().unwrap();
	let mut net = TestNet::new(2);
	net.peer(1).push_blocks(500, false);
	net.block_until_sync(&mut runtime);
	let peer1 = &net.peers()[1];
	assert!(net.peers()[0].blockchain_canon_equals(peer1));
}

#[test]
fn sync_no_common_longer_chain_fails() {
	let _ = ::env_logger::try_init();
	let mut runtime = current_thread::Runtime::new().unwrap();
	let mut net = TestNet::new(3);
	net.peer(0).push_blocks(20, true);
	net.peer(1).push_blocks(20, false);
	runtime.block_on(futures::future::poll_fn::<(), (), _>(|| -> Result<_, ()> {
		net.poll();
		if net.peer(0).is_major_syncing() {
			Ok(Async::NotReady)
		} else {
			Ok(Async::Ready(()))
		}
	})).unwrap();
	let peer1 = &net.peers()[1];
	assert!(!net.peers()[0].blockchain_canon_equals(peer1));
}

#[test]
fn sync_justifications() {
	let _ = ::env_logger::try_init();
	let mut runtime = current_thread::Runtime::new().unwrap();
	let mut net = JustificationTestNet::new(3);
	net.peer(0).push_blocks(20, false);
	net.block_until_sync(&mut runtime);

	// there's currently no justification for block #10
	assert_eq!(net.peer(0).client().justification(&BlockId::Number(10)).unwrap(), None);
	assert_eq!(net.peer(1).client().justification(&BlockId::Number(10)).unwrap(), None);

	// we finalize block #10, #15 and #20 for peer 0 with a justification
	net.peer(0).client().finalize_block(BlockId::Number(10), Some(Vec::new()), true).unwrap();
	net.peer(0).client().finalize_block(BlockId::Number(15), Some(Vec::new()), true).unwrap();
	net.peer(0).client().finalize_block(BlockId::Number(20), Some(Vec::new()), true).unwrap();

	let h1 = net.peer(1).client().header(&BlockId::Number(10)).unwrap().unwrap();
	let h2 = net.peer(1).client().header(&BlockId::Number(15)).unwrap().unwrap();
	let h3 = net.peer(1).client().header(&BlockId::Number(20)).unwrap().unwrap();

	// peer 1 should get the justifications from the network
	net.peer(1).request_justification(&h1.hash().into(), 10);
	net.peer(1).request_justification(&h2.hash().into(), 15);
	net.peer(1).request_justification(&h3.hash().into(), 20);

	runtime.block_on(futures::future::poll_fn::<(), (), _>(|| {
		net.poll();

		for height in (10..21).step_by(5) {
			if net.peer(0).client().justification(&BlockId::Number(height)).unwrap() != Some(Vec::new()) {
				return Ok(Async::NotReady);
			}
			if net.peer(1).client().justification(&BlockId::Number(height)).unwrap() != Some(Vec::new()) {
				return Ok(Async::NotReady);
			}
		}

		Ok(Async::Ready(()))
	})).unwrap();
}

#[test]
fn sync_justifications_across_forks() {
	let _ = ::env_logger::try_init();
	let mut runtime = current_thread::Runtime::new().unwrap();
	let mut net = JustificationTestNet::new(3);
	// we push 5 blocks
	net.peer(0).push_blocks(5, false);
	// and then two forks 5 and 6 blocks long
	let f1_best = net.peer(0).push_blocks_at(BlockId::Number(5), 5, false);
	let f2_best = net.peer(0).push_blocks_at(BlockId::Number(5), 6, false);

	// peer 1 will only see the longer fork. but we'll request justifications
	// for both and finalize the small fork instead.
	net.block_until_sync(&mut runtime);

	net.peer(0).client().finalize_block(BlockId::Hash(f1_best), Some(Vec::new()), true).unwrap();

	net.peer(1).request_justification(&f1_best, 10);
	net.peer(1).request_justification(&f2_best, 11);

	runtime.block_on(futures::future::poll_fn::<(), (), _>(|| {
		net.poll();

		if net.peer(0).client().justification(&BlockId::Number(10)).unwrap() == Some(Vec::new()) &&
			net.peer(1).client().justification(&BlockId::Number(10)).unwrap() == Some(Vec::new())
		{
			Ok(Async::Ready(()))
		} else {
			Ok(Async::NotReady)
		}
	})).unwrap();
}

#[test]
fn sync_after_fork_works() {
	let _ = ::env_logger::try_init();
	let mut runtime = current_thread::Runtime::new().unwrap();
	let mut net = TestNet::new(3);
	net.peer(0).push_blocks(30, false);
	net.peer(1).push_blocks(30, false);
	net.peer(2).push_blocks(30, false);

	net.peer(0).push_blocks(10, true);
	net.peer(1).push_blocks(20, false);
	net.peer(2).push_blocks(20, false);

	net.peer(1).push_blocks(10, true);
	net.peer(2).push_blocks(1, false);

	// peer 1 has the best chain
	net.block_until_sync(&mut runtime);
	let peer1 = &net.peers()[1];
	assert!(net.peers()[0].blockchain_canon_equals(peer1));
	(net.peers()[1].blockchain_canon_equals(peer1));
	(net.peers()[2].blockchain_canon_equals(peer1));
}

#[test]
fn syncs_all_forks() {
	let _ = ::env_logger::try_init();
	let mut runtime = current_thread::Runtime::new().unwrap();
	let mut net = TestNet::new(4);
	net.peer(0).push_blocks(2, false);
	net.peer(1).push_blocks(2, false);

	net.peer(0).push_blocks(2, true);
	net.peer(1).push_blocks(4, false);

	net.block_until_sync(&mut runtime);
	// Check that all peers have all of the blocks.
	assert_eq!(9, net.peer(0).blocks_count());
	assert_eq!(9, net.peer(1).blocks_count());
}

#[test]
fn own_blocks_are_announced() {
	let _ = ::env_logger::try_init();
	let mut runtime = current_thread::Runtime::new().unwrap();
	let mut net = TestNet::new(3);
	net.block_until_sync(&mut runtime); // connect'em
	net.peer(0).generate_blocks(1, BlockOrigin::Own, |builder| builder.bake().unwrap());

	net.block_until_sync(&mut runtime);

	assert_eq!(net.peer(0).client.info().chain.best_number, 1);
	assert_eq!(net.peer(1).client.info().chain.best_number, 1);
	let peer0 = &net.peers()[0];
	assert!(net.peers()[1].blockchain_canon_equals(peer0));
	(net.peers()[2].blockchain_canon_equals(peer0));
}

#[test]
fn blocks_are_not_announced_by_light_nodes() {
	let _ = ::env_logger::try_init();
	let mut runtime = current_thread::Runtime::new().unwrap();
	let mut net = TestNet::new(0);

	// full peer0 is connected to light peer
	// light peer1 is connected to full peer2
	let mut light_config = ProtocolConfig::default();
	light_config.roles = Roles::LIGHT;
	net.add_full_peer(&ProtocolConfig::default());
	net.add_light_peer(&light_config);

	// Sync between 0 and 1.
	net.peer(0).push_blocks(1, false);
	assert_eq!(net.peer(0).client.info().chain.best_number, 1);
	net.block_until_sync(&mut runtime);
	assert_eq!(net.peer(1).client.info().chain.best_number, 1);

	// Add another node and remove node 0.
	net.add_full_peer(&ProtocolConfig::default());
	net.peers.remove(0);

	// Poll for a few seconds and make sure 1 and 2 (now 0 and 1) don't sync together.
	let mut delay = futures_timer::Delay::new(Duration::from_secs(5)).compat();
	runtime.block_on(futures::future::poll_fn::<(), (), _>(|| {
		net.poll();
		delay.poll().map_err(|_| ())
	})).unwrap();
	assert_eq!(net.peer(1).client.info().chain.best_number, 0);
}

#[test]
fn can_sync_small_non_best_forks() {
	let _ = ::env_logger::try_init();
	let mut runtime = current_thread::Runtime::new().unwrap();
	let mut net = TestNet::new(2);
	net.peer(0).push_blocks(30, false);
	net.peer(1).push_blocks(30, false);

	// small fork + reorg on peer 1.
	net.peer(0).push_blocks_at(BlockId::Number(30), 2, true);
	let small_hash = net.peer(0).client().info().chain.best_hash;
	net.peer(0).push_blocks_at(BlockId::Number(30), 10, false);
	assert_eq!(net.peer(0).client().info().chain.best_number, 40);

	// peer 1 only ever had the long fork.
	net.peer(1).push_blocks(10, false);
	assert_eq!(net.peer(1).client().info().chain.best_number, 40);

	assert!(net.peer(0).client().header(&BlockId::Hash(small_hash)).unwrap().is_some());
	assert!(net.peer(1).client().header(&BlockId::Hash(small_hash)).unwrap().is_none());

	// poll until the two nodes connect, otherwise announcing the block will not work
	runtime.block_on(futures::future::poll_fn::<(), (), _>(|| -> Result<_, ()> {
		net.poll();
		if net.peer(0).num_peers() == 0 {
			Ok(Async::NotReady)
		} else {
			Ok(Async::Ready(()))
		}
	})).unwrap();

	// synchronization: 0 synced to longer chain and 1 didn't sync to small chain.

	assert_eq!(net.peer(0).client().info().chain.best_number, 40);

	assert!(net.peer(0).client().header(&BlockId::Hash(small_hash)).unwrap().is_some());
	assert!(!net.peer(1).client().header(&BlockId::Hash(small_hash)).unwrap().is_some());

	net.peer(0).announce_block(small_hash, Vec::new());

	// after announcing, peer 1 downloads the block.

	runtime.block_on(futures::future::poll_fn::<(), (), _>(|| -> Result<_, ()> {
		net.poll();

		assert!(net.peer(0).client().header(&BlockId::Hash(small_hash)).unwrap().is_some());
		if net.peer(1).client().header(&BlockId::Hash(small_hash)).unwrap().is_none() {
			return Ok(Async::NotReady)
		}
		Ok(Async::Ready(()))
	})).unwrap();
	net.block_until_sync(&mut runtime);

	let another_fork = net.peer(0).push_blocks_at(BlockId::Number(35), 2, true);
	net.peer(0).announce_block(another_fork, Vec::new());
	runtime.block_on(futures::future::poll_fn::<(), (), _>(|| -> Result<_, ()> {
		net.poll();
		if net.peer(1).client().header(&BlockId::Hash(another_fork)).unwrap().is_none() {
			return Ok(Async::NotReady)
		}
		Ok(Async::Ready(()))
	})).unwrap();
}

#[test]
fn can_not_sync_from_light_peer() {
	let _ = ::env_logger::try_init();
	let mut runtime = current_thread::Runtime::new().unwrap();

	// given the network with 1 full nodes (#0) and 1 light node (#1)
	let mut net = TestNet::new(1);
	net.add_light_peer(&Default::default());

	// generate some blocks on #0
	net.peer(0).push_blocks(1, false);

	// and let the light client sync from this node
	net.block_until_sync(&mut runtime);

	// ensure #0 && #1 have the same best block
	let full0_info = net.peer(0).client.info().chain;
	let light_info = net.peer(1).client.info().chain;
	assert_eq!(full0_info.best_number, 1);
	assert_eq!(light_info.best_number, 1);
	assert_eq!(light_info.best_hash, full0_info.best_hash);

	// add new full client (#2) && remove #0
	net.add_full_peer(&Default::default());
	net.peers.remove(0);

	// ensure that the #2 (now #1) fails to sync block #1 even after 5 seconds
	let mut test_finished = futures_timer::Delay::new(Duration::from_secs(5)).compat();
	runtime.block_on(futures::future::poll_fn::<(), (), _>(|| -> Result<_, ()> {
		net.poll();
		test_finished.poll().map_err(|_| ())
	})).unwrap();
}

#[test]
fn light_peer_imports_header_from_announce() {
	let _ = ::env_logger::try_init();
	let mut runtime = current_thread::Runtime::new().unwrap();

	fn import_with_announce(net: &mut TestNet, runtime: &mut current_thread::Runtime, hash: H256) {
		net.peer(0).announce_block(hash, Vec::new());

		runtime.block_on(futures::future::poll_fn::<(), (), _>(|| {
			net.poll();
			if net.peer(1).client().header(&BlockId::Hash(hash)).unwrap().is_some() {
				Ok(Async::Ready(()))
			} else {
				Ok(Async::NotReady)
			}
		})).unwrap();
	}

	// given the network with 1 full nodes (#0) and 1 light node (#1)
	let mut net = TestNet::new(1);
	net.add_light_peer(&Default::default());

	// let them connect to each other
	net.block_until_sync(&mut runtime);

	// check that NEW block is imported from announce message
	let new_hash = net.peer(0).push_blocks(1, false);
	import_with_announce(&mut net, &mut runtime, new_hash);

	// check that KNOWN STALE block is imported from announce message
	let known_stale_hash = net.peer(0).push_blocks_at(BlockId::Number(0), 1, true);
	import_with_announce(&mut net, &mut runtime, known_stale_hash);
}

#[test]
fn can_sync_explicit_forks() {
	let _ = ::env_logger::try_init();
	let mut runtime = current_thread::Runtime::new().unwrap();
	let mut net = TestNet::new(2);
	net.peer(0).push_blocks(30, false);
	net.peer(1).push_blocks(30, false);

	// small fork + reorg on peer 1.
	net.peer(0).push_blocks_at(BlockId::Number(30), 2, true);
	let small_hash = net.peer(0).client().info().chain.best_hash;
	let small_number = net.peer(0).client().info().chain.best_number;
	net.peer(0).push_blocks_at(BlockId::Number(30), 10, false);
	assert_eq!(net.peer(0).client().info().chain.best_number, 40);

	// peer 1 only ever had the long fork.
	net.peer(1).push_blocks(10, false);
	assert_eq!(net.peer(1).client().info().chain.best_number, 40);

	assert!(net.peer(0).client().header(&BlockId::Hash(small_hash)).unwrap().is_some());
	assert!(net.peer(1).client().header(&BlockId::Hash(small_hash)).unwrap().is_none());

	// poll until the two nodes connect, otherwise announcing the block will not work
	runtime.block_on(futures::future::poll_fn::<(), (), _>(|| -> Result<_, ()> {
		net.poll();
		if net.peer(0).num_peers() == 0  || net.peer(1).num_peers() == 0 {
			Ok(Async::NotReady)
		} else {
			Ok(Async::Ready(()))
		}
	})).unwrap();

	// synchronization: 0 synced to longer chain and 1 didn't sync to small chain.

	assert_eq!(net.peer(0).client().info().chain.best_number, 40);

	assert!(net.peer(0).client().header(&BlockId::Hash(small_hash)).unwrap().is_some());
	assert!(!net.peer(1).client().header(&BlockId::Hash(small_hash)).unwrap().is_some());

	// request explicit sync
	let first_peer_id = net.peer(0).id();
	net.peer(1).set_sync_fork_request(vec![first_peer_id], small_hash, small_number);

	// peer 1 downloads the block.
	runtime.block_on(futures::future::poll_fn::<(), (), _>(|| -> Result<_, ()> {
		net.poll();

		assert!(net.peer(0).client().header(&BlockId::Hash(small_hash)).unwrap().is_some());
		if net.peer(1).client().header(&BlockId::Hash(small_hash)).unwrap().is_none() {
			return Ok(Async::NotReady)
		}
		Ok(Async::Ready(()))
	})).unwrap();
}

#[test]
fn syncs_header_only_forks() {
	let _ = ::env_logger::try_init();
	let mut runtime = current_thread::Runtime::new().unwrap();
	let mut net = TestNet::new(0);
	let config = ProtocolConfig::default();
	net.add_full_peer_with_states(&config, None);
	net.add_full_peer_with_states(&config, Some(3));
	net.peer(0).push_blocks(2, false);
	net.peer(1).push_blocks(2, false);

	net.peer(0).push_blocks(2, true);
	let small_hash = net.peer(0).client().info().chain.best_hash;
	let small_number = net.peer(0).client().info().chain.best_number;
	net.peer(1).push_blocks(4, false);

	net.block_until_sync(&mut runtime);
	// Peer 1 will sync the small fork even though common block state is missing
	assert_eq!(9, net.peer(0).blocks_count());
	assert_eq!(9, net.peer(1).blocks_count());

	// Request explicit header-only sync request for the ancient fork.
	let first_peer_id = net.peer(0).id();
	net.peer(1).set_sync_fork_request(vec![first_peer_id], small_hash, small_number);
	runtime.block_on(futures::future::poll_fn::<(), (), _>(|| -> Result<_, ()> {
		net.poll();
		if net.peer(1).client().header(&BlockId::Hash(small_hash)).unwrap().is_none() {
			return Ok(Async::NotReady)
		}
		Ok(Async::Ready(()))
	})).unwrap();
}

#[test]
fn does_not_sync_announced_old_best_block() {
	let _ = ::env_logger::try_init();
	let mut runtime = current_thread::Runtime::new().unwrap();
	let mut net = TestNet::new(3);

	let old_hash = net.peer(0).push_blocks(1, false);
	let old_hash_with_parent = net.peer(0).push_blocks(1, false);
	net.peer(0).push_blocks(18, true);
	net.peer(1).push_blocks(20, true);

	net.peer(0).announce_block(old_hash, Vec::new());
	runtime.block_on(futures::future::poll_fn::<(), (), _>(|| -> Result<_, ()> {
		// poll once to import announcement
		net.poll();
		Ok(Async::Ready(()))
	})).unwrap();
	assert!(!net.peer(1).is_major_syncing());

	net.peer(0).announce_block(old_hash_with_parent, Vec::new());
	runtime.block_on(futures::future::poll_fn::<(), (), _>(|| -> Result<_, ()> {
		// poll once to import announcement
		net.poll();
		Ok(Async::Ready(()))
	})).unwrap();
	assert!(!net.peer(1).is_major_syncing());
}
