#[cfg(not(feature = "minimal"))]
use crate::{
	compute_period, config,
	config::{SYNC_COMMITTEE_BITS_SIZE, SYNC_COMMITTEE_SIZE},
	mock::*,
	Error, ExecutionHeaders, FinalizedBeaconHeaders, FinalizedBeaconHeadersBlockRoot,
	FinalizedHeaderState, LatestFinalizedHeader, LatestSyncCommitteePeriod, ValidatorsRoot,
};
use frame_support::{assert_err, assert_ok};
use hex_literal::hex;
use primitives::{
	decompress_sync_committee_bits, fast_aggregate_verify_legacy, prepare_g1_pubkeys,
};
use sp_core::H256;

#[test]
fn it_syncs_from_an_initial_checkpoint() {
	let initial_sync = get_initial_sync::<SYNC_COMMITTEE_SIZE>();

	new_tester::<mock_mainnet::Test>().execute_with(|| {
		assert_ok!(mock_mainnet::EthereumBeaconClient::process_checkpoint_update(&initial_sync));

		let block_root: H256 = initial_sync.header.hash_tree_root().unwrap();

		assert!(<FinalizedBeaconHeaders<mock_mainnet::Test>>::contains_key(block_root));
	});
}

#[test]
fn it_updates_a_committee_period_sync_update() {
	let update =
		get_committee_sync_period_update::<SYNC_COMMITTEE_SIZE, SYNC_COMMITTEE_BITS_SIZE>();
	let current_sync_committee =
		get_initial_sync::<{ SYNC_COMMITTEE_SIZE }>().current_sync_committee;
	let current_period = compute_period(update.attested_header.slot);

	new_tester::<mock_mainnet::Test>().execute_with(|| {
		LatestSyncCommitteePeriod::<mock_mainnet::Test>::set(current_period);
		ValidatorsRoot::<mock_mainnet::Test>::set(get_validators_root::<{ SYNC_COMMITTEE_SIZE }>());
		assert_ok!(mock_mainnet::EthereumBeaconClient::store_sync_committee(
			current_period,
			&current_sync_committee,
		));

		let block_root: H256 = update.finalized_header.hash_tree_root().unwrap();

		assert_ok!(mock_mainnet::EthereumBeaconClient::sync_committee_period_update(
			mock_mainnet::RuntimeOrigin::signed(1),
			update,
		));

		assert!(<FinalizedBeaconHeaders<mock_mainnet::Test>>::contains_key(block_root));
	});
}

#[test]
fn it_processes_a_finalized_header_update() {
	let update = get_finalized_header_update::<SYNC_COMMITTEE_SIZE, SYNC_COMMITTEE_BITS_SIZE>();
	let current_sync_committee =
		get_initial_sync::<{ SYNC_COMMITTEE_SIZE }>().current_sync_committee;
	let current_period = compute_period(update.attested_header.slot);

	let slot = update.finalized_header.slot;
	let import_time = 1616508000u64 + (slot * config::SECONDS_PER_SLOT as u64); // Goerli genesis time +
	let mock_pallet_time = import_time + 3600; // plus one hour

	new_tester::<mock_mainnet::Test>().execute_with(|| {
		mock_mainnet::Timestamp::set_timestamp(mock_pallet_time * 1000); // needs to be in milliseconds
		assert_ok!(mock_mainnet::EthereumBeaconClient::store_sync_committee(
			current_period,
			&current_sync_committee,
		));
		LatestFinalizedHeader::<mock_mainnet::Test>::set(FinalizedHeaderState {
			beacon_block_root: Default::default(),
			import_time,
			beacon_slot: slot - 1,
		});
		ValidatorsRoot::<mock_mainnet::Test>::set(get_validators_root::<{ SYNC_COMMITTEE_SIZE }>());

		assert_ok!(mock_mainnet::EthereumBeaconClient::import_finalized_header(
			mock_mainnet::RuntimeOrigin::signed(1),
			update.clone()
		));

		let block_root: H256 = update.finalized_header.hash_tree_root().unwrap();

		assert!(<FinalizedBeaconHeaders<mock_mainnet::Test>>::contains_key(block_root));
	});
}

#[test]
fn it_errors_when_weak_subjectivity_period_exceeded_for_a_finalized_header_update() {
	let update = get_finalized_header_update::<SYNC_COMMITTEE_SIZE, SYNC_COMMITTEE_BITS_SIZE>();
	let current_sync_committee = get_initial_sync::<SYNC_COMMITTEE_SIZE>().current_sync_committee;

	let current_period = compute_period(update.attested_header.slot);

	let slot = update.finalized_header.slot;
	let import_time = 1616508000u64 + (slot * config::SECONDS_PER_SLOT as u64);
	let mock_pallet_time = import_time + 100800; // plus 28 hours

	new_tester::<mock_mainnet::Test>().execute_with(|| {
		mock_mainnet::Timestamp::set_timestamp(mock_pallet_time * 1000); // needs to be in milliseconds
		assert_ok!(mock_mainnet::EthereumBeaconClient::store_sync_committee(
			current_period,
			&current_sync_committee,
		));
		LatestFinalizedHeader::<mock_mainnet::Test>::set(FinalizedHeaderState {
			beacon_block_root: Default::default(),
			import_time,
			beacon_slot: slot - 1,
		});
		ValidatorsRoot::<mock_mainnet::Test>::set(get_validators_root::<SYNC_COMMITTEE_SIZE>());

		assert_err!(
			mock_mainnet::EthereumBeaconClient::import_finalized_header(
				mock_mainnet::RuntimeOrigin::signed(1),
				update.clone()
			),
			Error::<mock_mainnet::Test>::BridgeBlocked
		);
	});
}

#[test]
fn it_processes_a_header_update() {
	let update = get_header_update();
	let current_sync_committee =
		get_initial_sync::<{ config::SYNC_COMMITTEE_SIZE }>().current_sync_committee;
	let current_period = compute_period(update.header.slot);

	let finalized_update =
		get_finalized_header_update::<SYNC_COMMITTEE_SIZE, SYNC_COMMITTEE_BITS_SIZE>();
	let finalized_slot = finalized_update.finalized_header.slot;
	let finalized_block_root: H256 = finalized_update.finalized_header.hash_tree_root().unwrap();

	new_tester::<mock_mainnet::Test>().execute_with(|| {
		assert_ok!(mock_mainnet::EthereumBeaconClient::store_sync_committee(
			current_period,
			&current_sync_committee,
		));
		ValidatorsRoot::<mock_mainnet::Test>::set(get_validators_root::<SYNC_COMMITTEE_SIZE>());
		LatestFinalizedHeader::<mock_mainnet::Test>::set(FinalizedHeaderState {
			beacon_block_root: finalized_block_root,
			beacon_slot: finalized_slot,
			import_time: 0,
		});
		FinalizedBeaconHeadersBlockRoot::<mock_mainnet::Test>::insert(
			finalized_block_root,
			finalized_update.block_roots_root,
		);

		assert_ok!(mock_mainnet::EthereumBeaconClient::import_execution_header(
			mock_mainnet::RuntimeOrigin::signed(1),
			update.clone()
		));

		assert!(<ExecutionHeaders<mock_mainnet::Test>>::contains_key(
			update.execution_header.block_hash
		));
	});
}

#[test]
pub fn test_hash_tree_root_sync_committee() {
	let sync_committee = get_committee_sync_ssz_test_data::<SYNC_COMMITTEE_SIZE>();
	let hash_root_result = sync_committee.hash_tree_root();
	assert_ok!(&hash_root_result);

	let hash_root: H256 = hash_root_result.unwrap().into();
	assert_eq!(
		hash_root,
		hex!("99daf976424b62249669bc842e9b8e5a5a2960d1d81d98c3267f471409c3c841").into()
	);
}

#[test]
pub fn test_bls_fast_aggregate_verify() {
	let test_data =
		get_bls_signature_verify_test_data::<SYNC_COMMITTEE_SIZE, SYNC_COMMITTEE_BITS_SIZE>();

	let milagro_pubkeys = prepare_g1_pubkeys(&test_data.pubkeys.to_vec()).unwrap();

	let participant_bits = decompress_sync_committee_bits::<
		SYNC_COMMITTEE_SIZE,
		SYNC_COMMITTEE_BITS_SIZE,
	>(test_data.sync_committee_bits);

	let participant_pubkeys =
		mock_mainnet::EthereumBeaconClient::find_pubkeys(&participant_bits, &milagro_pubkeys, true);

	let signing_root = mock_mainnet::EthereumBeaconClient::signing_root(
		&test_data.header,
		test_data.validators_root,
		test_data.signature_slot,
	)
	.unwrap();

	assert_ok!(fast_aggregate_verify_legacy(
		&participant_pubkeys,
		signing_root,
		&test_data.sync_committee_signature,
	));
}
