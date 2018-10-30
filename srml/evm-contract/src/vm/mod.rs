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

//! Virtual machines support library

pub(crate) mod action_params;
pub(crate) mod call_type;
pub(crate) mod env_info;
pub(crate) mod schedule;
pub(crate) mod ext;
pub(crate) mod return_data;
pub(crate) mod error;

pub mod tests;

pub use self::schedule::Schedule;
pub use self::action_params::{ActionParams, ActionValue};
pub use self::env_info::EnvInfo;
pub use self::call_type::CallType;
pub use self::return_data::ReturnData;
pub use self::error::TrapError;
use self::ext::{Ext, MessageCallResult, ContractCreateResult};
use self::error::ExecTrapResult;
use self::return_data::GasLeft;

/// Virtual Machine interface
pub trait Exec: Send {
	/// This function should be used to execute transaction.
	/// It returns either an error, a known amount of gas left, or parameters to be used
	/// to compute the final gas left.
	fn exec(self: Box<Self>, ext: &mut Ext) -> ExecTrapResult<GasLeft>;
}

/// Resume call interface
pub trait ResumeCall: Send {
	/// Resume an execution for call, returns back the Vm interface.
	fn resume_call(self: Box<Self>, result: MessageCallResult) -> Box<Exec>;
}

/// Resume create interface
pub trait ResumeCreate: Send {
	/// Resume an execution from create, returns back the Vm interface.
	fn resume_create(self: Box<Self>, result: ContractCreateResult) -> Box<Exec>;
}

pub fn create<AccountId, >
