// Copyright 2015-2018 Parity Technologies (UK) Ltd.
// This file is part of Parity.

// Parity is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Parity is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Parity.  If not, see <http://www.gnu.org/licenses/>.

//! Transaction Execution environment.
use std::cmp;
use std::sync::Arc;
use ethereum_types::{H256, U256, Address};
use bytes::Bytes;
use executive::*;
use vm::{
	self, ActionParams, ActionValue, EnvInfo, CallType, Schedule,
	Ext, ContractCreateResult, MessageCallResult, CreateContractAddress,
	ReturnData, TrapKind
};
use substate::Substate;
use transaction::UNSIGNED_SENDER;
use trace::{Tracer, VMTracer};

/// Policy for handling output data on `RETURN` opcode.
pub enum OutputPolicy {
	/// Return reference to fixed sized output.
	/// Used for message calls.
	Return,
	/// Init new contract as soon as `RETURN` is called.
	InitContract,
}

/// Transaction properties that externalities need to know about.
pub struct OriginInfo<AccountId: Clone> {
	address: AccountId,
	origin: AccountId,
	gas_price: U256,
	value: U256,
}

impl<AccountId: Clone> OriginInfo<AccountId> {
	/// Populates origin info from action params.
	pub fn from(params: &ActionParams<AccountId>) -> Self {
		OriginInfo {
			address: params.address.clone(),
			origin: params.origin.clone(),
			gas_price: params.gas_price,
			value: match params.value {
				ActionValue::Transfer(val) | ActionValue::Apparent(val) => val
			},
		}
	}
}

/// Implementation of evm Externalities.
pub struct Externalities<'a, B: 'a, AccountId: Clone> {
	state: &'a mut State<B>,
	env_info: &'a EnvInfo,
	depth: usize,
	stack_depth: usize,
	origin_info: &'a OriginInfo<AccountId>,
	substate: &'a mut Substate,
	schedule: &'a Schedule,
	output: OutputPolicy,
	static_flag: bool,
}

impl<'a, B: 'a, AccountId> Externalities<'a, B, AccountId>
	where B: StateBackend, AccountId: Clone
{
	/// Basic `Externalities` constructor.
	pub fn new(
		state: &'a mut State<B>,
		env_info: &'a EnvInfo,
		schedule: &'a Schedule,
		depth: usize,
		stack_depth: usize,
		origin_info: &'a OriginInfo<AccountId>,
		substate: &'a mut Substate,
		output: OutputPolicy,
		static_flag: bool,
	) -> Self {
		Externalities {
			state: state,
			env_info: env_info,
			depth: depth,
			stack_depth: stack_depth,
			origin_info: origin_info,
			substate: substate,
			schedule: schedule,
			output: output,
			static_flag: static_flag,
		}
	}
}

impl<'a, B: 'a, AccountId> Ext for Externalities<'a, B, AccountId>
	where B: StateBackend, AccountId: Clone
{
	fn initial_storage_at(&self, key: &H256) -> vm::Result<H256> {
		self.state.checkpoint_storage_at(0, &self.origin_info.address, key).map(|v| v.unwrap_or(H256::zero())).map_err(Into::into)
	}

	fn storage_at(&self, key: &H256) -> vm::Result<H256> {
		self.state.storage_at(&self.origin_info.address, key).map_err(Into::into)
	}

	fn set_storage(&mut self, key: H256, value: H256) -> vm::Result<()> {
		if self.static_flag {
			Err(vm::Error::MutableCallInStaticContext)
		} else {
			self.state.set_storage(&self.origin_info.address, key, value).map_err(Into::into)
		}
	}

	fn is_static(&self) -> bool {
		return self.static_flag
	}

	fn exists(&self, address: &AccountId) -> vm::Result<bool> {
		self.state.exists(address).map_err(Into::into)
	}

	fn exists_and_not_null(&self, address: &AccountId) -> vm::Result<bool> {
		self.state.exists_and_not_null(address).map_err(Into::into)
	}

	fn origin_balance(&self) -> vm::Result<U256> {
		self.balance(&self.origin_info.address).map_err(Into::into)
	}

	fn balance(&self, address: &AccountId) -> vm::Result<U256> {
		self.state.balance(address).map_err(Into::into)
	}

	fn blockhash(&mut self, number: &U256) -> H256 {
		unimplemented!()
	}

	fn create(
		&mut self,
		gas: &U256,
		value: &U256,
		code: &[u8],
		address_scheme: CreateContractAddress,
		trap: bool,
	) -> ::std::result::Result<ContractCreateResult, TrapKind> {
		// create new contract address
		let (address, code_hash) = match self.state.nonce(&self.origin_info.address) {
			Ok(nonce) => contract_address(address_scheme, &self.origin_info.address, &nonce, &code),
			Err(e) => {
				debug!(target: "ext", "Database corruption encountered: {:?}", e);
				return Ok(ContractCreateResult::Failed)
			}
		};

		// prepare the params
		let params = ActionParams {
			code_address: address.clone(),
			address: address.clone(),
			sender: self.origin_info.address.clone(),
			origin: self.origin_info.origin.clone(),
			gas: *gas,
			gas_price: self.origin_info.gas_price,
			value: ActionValue::Transfer(*value),
			code: Some(Arc::new(code.to_vec())),
			code_hash: code_hash,
			data: None,
			call_type: CallType::None,
			params_type: vm::ParamsType::Embedded,
		};

		if !self.static_flag {
			if !self.schedule.keep_unsigned_nonce || params.sender != UNSIGNED_SENDER {
				if let Err(e) = self.state.inc_nonce(&self.origin_info.address) {
					debug!(target: "ext", "Database corruption encountered: {:?}", e);
					return Ok(ContractCreateResult::Failed)
				}
			}
		}

		if trap {
			return Err(TrapKind::Create(params, address));
		}

		// TODO: handle internal error separately
		let mut ex = Executive::from_parent(self.state, self.env_info, self.schedule, self.depth, self.static_flag);
		let out = ex.create_with_crossbeam(params, self.substate, self.stack_depth + 1);
		Ok(into_contract_create_result(out, &address, self.substate))
	}

	fn call(
		&mut self,
		gas: &U256,
		sender_address: &AccountId,
		receive_address: &AccountId,
		value: Option<U256>,
		data: &[u8],
		code_address: &AccountId,
		call_type: CallType,
		trap: bool,
	) -> ::std::result::Result<MessageCallResult, TrapKind> {
		trace!(target: "externalities", "call");

		let code_res = self.state.code(code_address)
			.and_then(|code| self.state.code_hash(code_address).map(|hash| (code, hash)));

		let (code, code_hash) = match code_res {
			Ok((code, hash)) => (code, hash),
			Err(_) => return Ok(MessageCallResult::Failed),
		};

		let mut params = ActionParams {
			sender: sender_address.clone(),
			address: receive_address.clone(),
			value: ActionValue::Apparent(self.origin_info.value),
			code_address: code_address.clone(),
			origin: self.origin_info.origin.clone(),
			gas: *gas,
			gas_price: self.origin_info.gas_price,
			code: code,
			code_hash: code_hash,
			data: Some(data.to_vec()),
			call_type: call_type,
			params_type: vm::ParamsType::Separate,
		};

		if let Some(value) = value {
			params.value = ActionValue::Transfer(value);
		}

		if trap {
			return Err(TrapKind::Call(params));
		}

		let mut ex = Executive::from_parent(self.state, self.env_info, self.schedule, self.depth, self.static_flag);
		let out = ex.call_with_crossbeam(params, self.substate, self.stack_depth + 1);
		Ok(into_message_call_result(out))
	}

	fn extcode(&self, address: &AccountId) -> vm::Result<Option<Arc<Bytes>>> {
		Ok(self.state.code(address)?)
	}

	fn extcodehash(&self, address: &AccountId) -> vm::Result<Option<H256>> {
		Ok(self.state.code_hash(address)?)
	}

	fn extcodesize(&self, address: &AccountId) -> vm::Result<Option<usize>> {
		Ok(self.state.code_size(address)?)
	}

	fn ret(self, gas: &U256, data: &ReturnData, apply_state: bool) -> vm::Result<U256>
		where Self: Sized {
		match self.output {
			OutputPolicy::Return => {
				Ok(*gas)
			},
			OutputPolicy::InitContract if apply_state => {
				let return_cost = U256::from(data.len()) * U256::from(self.schedule.create_data_gas);
				if return_cost > *gas || data.len() > self.schedule.create_data_limit {
					return match self.schedule.exceptional_failed_code_deposit {
						true => Err(vm::Error::OutOfGas),
						false => Ok(*gas)
					}
				}
				self.state.init_code(&self.origin_info.address, data.to_vec())?;
				Ok(*gas - return_cost)
			},
			OutputPolicy::InitContract => {
				Ok(*gas)
			},
		}
	}

	fn log(&mut self, topics: Vec<H256>, data: &[u8]) -> vm::Result<()> {
		use log_entry::LogEntry;

		if self.static_flag {
			return Err(vm::Error::MutableCallInStaticContext);
		}

		let address = self.origin_info.address.clone();
		self.substate.logs.push(LogEntry {
			address: address,
			topics: topics,
			data: data.to_vec()
		});

		Ok(())
	}

	fn suicide(&mut self, refund_address: &AccountId) -> vm::Result<()> {
		if self.static_flag {
			return Err(vm::Error::MutableCallInStaticContext);
		}

		let address = self.origin_info.address.clone();
		let balance = self.balance(&address)?;
		if &address == refund_address {
			// TODO [todr] To be consistent with CPP client we set balance to 0 in that case.
			self.state.sub_balance(&address, &balance, &mut CleanupMode::NoEmpty)?;
		} else {
			trace!(target: "ext", "Suiciding {} -> {} (xfer: {})", address, refund_address, balance);
			self.state.transfer_balance(
				&address,
				refund_address,
				&balance,
				self.substate.to_cleanup_mode(&self.schedule)
			)?;
		}

		self.substate.suicides.insert(address);

		Ok(())
	}

	fn schedule(&self) -> &Schedule {
		&self.schedule
	}

	fn env_info(&self) -> &EnvInfo {
		self.env_info
	}

	fn depth(&self) -> usize {
		self.depth
	}

	fn add_sstore_refund(&mut self, value: usize) {
		self.substate.sstore_clears_refund += value as i128;
	}

	fn sub_sstore_refund(&mut self, value: usize) {
		self.substate.sstore_clears_refund -= value as i128;
	}
}

/*
#[cfg(test)]
mod tests {
	use ethereum_types::{U256, Address};
	use evm::{EnvInfo, Ext, CallType};
	use state::{State, Substate};
	use test_helpers::get_temp_state;
	use super::*;
	use trace::{NoopTracer, NoopVMTracer};

	fn get_test_origin() -> OriginInfo {
		OriginInfo {
			address: Address::zero(),
			origin: Address::zero(),
			gas_price: U256::zero(),
			value: U256::zero()
		}
	}

	fn get_test_env_info() -> EnvInfo {
		EnvInfo {
			number: 100,
			author: 0.into(),
			timestamp: 0,
			difficulty: 0.into(),
			last_hashes: Arc::new(vec![]),
			gas_used: 0.into(),
			gas_limit: 0.into(),
		}
	}

	struct TestSetup {
		state: State<::state_db::StateDB>,
		machine: ::machine::EthereumMachine,
		schedule: Schedule,
		sub_state: Substate,
		env_info: EnvInfo
	}

	impl Default for TestSetup {
		fn default() -> Self {
			TestSetup::new()
		}
	}

	impl TestSetup {
		fn new() -> Self {
			let machine = ::spec::Spec::new_test_machine();
			let env_info = get_test_env_info();
			let schedule = machine.schedule(env_info.number);
			TestSetup {
				state: get_temp_state(),
				schedule: schedule,
				machine: machine,
				sub_state: Substate::new(),
				env_info: env_info,
			}
		}
	}

	#[test]
	fn can_be_created() {
		let mut setup = TestSetup::new();
		let state = &mut setup.state;
		let origin_info = get_test_origin();

		let ext = Externalities::new(state, &setup.env_info, &setup.machine, &setup.schedule, 0, 0, &origin_info, &mut setup.sub_state, OutputPolicy::InitContract, false);

		assert_eq!(ext.env_info().number, 100);
	}

	#[test]
	fn can_return_block_hash_no_env() {
		let mut setup = TestSetup::new();
		let state = &mut setup.state;
		let origin_info = get_test_origin();

		let mut ext = Externalities::new(state, &setup.env_info, &setup.machine, &setup.schedule, 0, 0, &origin_info, &mut setup.sub_state, OutputPolicy::InitContract, false);

		let hash = ext.blockhash(&"0000000000000000000000000000000000000000000000000000000000120000".parse::<U256>().unwrap());

		assert_eq!(hash, H256::zero());
	}

	#[test]
	fn can_return_block_hash() {
		let test_hash = H256::from("afafafafafafafafafafafbcbcbcbcbcbcbcbcbcbeeeeeeeeeeeeedddddddddd");
		let test_env_number = 0x120001;

		let mut setup = TestSetup::new();
		{
			let env_info = &mut setup.env_info;
			env_info.number = test_env_number;
			let mut last_hashes = (*env_info.last_hashes).clone();
			last_hashes.push(test_hash.clone());
			env_info.last_hashes = Arc::new(last_hashes);
		}
		let state = &mut setup.state;
		let origin_info = get_test_origin();

		let mut ext = Externalities::new(state, &setup.env_info, &setup.machine, &setup.schedule, 0, 0, &origin_info, &mut setup.sub_state, OutputPolicy::InitContract, false);

		let hash = ext.blockhash(&"0000000000000000000000000000000000000000000000000000000000120000".parse::<U256>().unwrap());

		assert_eq!(test_hash, hash);
	}

	#[test]
	#[should_panic]
	fn can_call_fail_empty() {
		let mut setup = TestSetup::new();
		let state = &mut setup.state;
		let origin_info = get_test_origin();

		let mut ext = Externalities::new(state, &setup.env_info, &setup.machine, &setup.schedule, 0, 0, &origin_info, &mut setup.sub_state, OutputPolicy::InitContract, false);

		// this should panic because we have no balance on any account
		ext.call(
			&"0000000000000000000000000000000000000000000000000000000000120000".parse::<U256>().unwrap(),
			&Address::new(),
			&Address::new(),
			Some("0000000000000000000000000000000000000000000000000000000000150000".parse::<U256>().unwrap()),
			&[],
			&Address::new(),
			CallType::Call,
			false,
		).ok().unwrap();
	}

	#[test]
	fn can_log() {
		let log_data = vec![120u8, 110u8];
		let log_topics = vec![H256::from("af0fa234a6af46afa23faf23bcbc1c1cb4bcb7bcbe7e7e7ee3ee2edddddddddd")];

		let mut setup = TestSetup::new();
		let state = &mut setup.state;
		let origin_info = get_test_origin();

		{
			let mut ext = Externalities::new(state, &setup.env_info, &setup.machine, &setup.schedule, 0, 0, &origin_info, &mut setup.sub_state, OutputPolicy::InitContract, false);
			ext.log(log_topics, &log_data).unwrap();
		}

		assert_eq!(setup.sub_state.logs.len(), 1);
	}

	#[test]
	fn can_suicide() {
		let refund_account = &Address::new();

		let mut setup = TestSetup::new();
		let state = &mut setup.state;
		let origin_info = get_test_origin();

		{
			let mut ext = Externalities::new(state, &setup.env_info, &setup.machine, &setup.schedule, 0, 0, &origin_info, &mut setup.sub_state, OutputPolicy::InitContract, false);
			ext.suicide(refund_account).unwrap();
		}

		assert_eq!(setup.sub_state.suicides.len(), 1);
	}

	#[test]
	fn can_create() {
		use std::str::FromStr;

		let mut setup = TestSetup::new();
		let state = &mut setup.state;
		let origin_info = get_test_origin();

		let address = {
			let mut ext = Externalities::new(state, &setup.env_info, &setup.machine, &setup.schedule, 0, 0, &origin_info, &mut setup.sub_state, OutputPolicy::InitContract, false);
			match ext.create(&U256::max_value(), &U256::zero(), &[], CreateContractAddress::FromSenderAndNonce, false) {
				Ok(ContractCreateResult::Created(address, _)) => address,
				_ => panic!("Test create failed; expected Created, got Failed/Reverted."),
			}
		};

		assert_eq!(address, Address::from_str("bd770416a3345f91e4b34576cb804a576fa48eb1").unwrap());
	}

	#[test]
	fn can_create2() {
		use std::str::FromStr;

		let mut setup = TestSetup::new();
		let state = &mut setup.state;
		let origin_info = get_test_origin();

		let address = {
			let mut ext = Externalities::new(state, &setup.env_info, &setup.machine, &setup.schedule, 0, 0, &origin_info, &mut setup.sub_state, OutputPolicy::InitContract, false);

			match ext.create(&U256::max_value(), &U256::zero(), &[], CreateContractAddress::FromSenderSaltAndCodeHash(H256::default()), false) {
				Ok(ContractCreateResult::Created(address, _)) => address,
				_ => panic!("Test create failed; expected Created, got Failed/Reverted."),
			}
		};

		assert_eq!(address, Address::from_str("e33c0c7f7df4809055c3eba6c09cfe4baf1bd9e0").unwrap());
	}
}
*/
