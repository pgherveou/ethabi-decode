// Copyright 2015-2020 Parity Technologies
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Contract event.
use tiny_keccak::Keccak;
use sp_std::collections::btree_map::BTreeMap;
use sp_std::prelude::*;

use crate::{
	decode, Error, H256, ParamType, Result, Token,
};


/// Event param specification.
#[derive(Debug, Clone, PartialEq)]
pub struct Param {
	/// Param type.
	pub kind: ParamType,
	/// Indexed flag. If true, param is used to build block bloom.
	pub indexed: bool,}


/// Contract event.
#[derive(Clone, Debug, PartialEq)]
pub struct Event {
	/// Event signature. Like "Foo(int32,bytes)".
	pub signature: &'static str,
	/// Event input.
	pub inputs: Vec<Param>,
	/// If anonymous, event cannot be found using `from` filter.
	pub anonymous: bool,
}

impl Event {

	fn signature_keccak256(&self) -> H256 {
		let mut result = [0u8; 32];
		let mut sponge = Keccak::new_keccak256();
		sponge.update(self.signature.as_ref());
		sponge.finalize(&mut result);
		result.into()
	}

	/// Returns all params of the event.
	fn indexed_params(&self, indexed: bool) -> Vec<Param> {
		self.inputs.iter().filter(|p| p.indexed == indexed).cloned().collect()
	}

	fn indices(&self, indexed: bool) -> Vec<usize> {
		self.inputs.iter().enumerate().filter(|(_, p)| p.indexed == indexed).map(|(i, _)| i).collect()
	}


	// Converts param types for indexed parameters to bytes32 where appropriate
	// This applies to strings, arrays, structs and bytes to follow the encoding of
	// these indexed param types according to
	// https://solidity.readthedocs.io/en/develop/abi-spec.html#encoding-of-indexed-event-parameters
	fn convert_topic_param_type(&self, kind: &ParamType) -> ParamType {
		match kind {
			ParamType::String
			| ParamType::Bytes
			| ParamType::Array(_)
			| ParamType::FixedArray(_, _)
			| ParamType::Tuple(_) => ParamType::FixedBytes(32),
			_ => kind.clone(),
		}
	}

	pub fn decode(&mut self, topics: Vec<H256>, data: Vec<u8>) -> Result<Vec<Token>> {
	
		let topics_len = topics.len();
		// obtains all params info
		let topic_params_index = self.indices(true);
		let data_params_index = self.indices(false);

		let topic_params = self.indexed_params(true);
		let data_params = self.indexed_params(false);
		// then take first topic if event is not anonymous
		let to_skip = if self.anonymous {
			0
		} else {
			// verify
			let event_signature = topics.get(0).ok_or(Error::InvalidData)?;
			if event_signature != &self.signature_keccak256() {
				return Err(Error::InvalidData.into());
			}
			1
		};

		let topic_types =
			topic_params.iter().map(|p| self.convert_topic_param_type(&p.kind)).collect::<Vec<ParamType>>();

		let flat_topics = topics.into_iter().skip(to_skip).flat_map(|t| t.as_ref().to_vec()).collect::<Vec<u8>>();

		let topic_tokens = decode(&topic_types, &flat_topics)?;

		// topic may be only a 32 bytes encoded token
		if topic_tokens.len() != topics_len - to_skip {
			return Err(Error::InvalidData);
		}

		let topics_named_tokens = topic_params_index.into_iter().zip(topic_tokens.into_iter());

		let data_types = data_params.iter().map(|p| p.kind.clone()).collect::<Vec<ParamType>>();
		let data_tokens = decode(&data_types, &data)?;
		let data_named_tokens = data_params_index.into_iter().zip(data_tokens.into_iter());

		let named_tokens = topics_named_tokens.chain(data_named_tokens).collect::<BTreeMap<usize, Token>>();

		let tokens: Vec<Token> = self
			.inputs.iter().enumerate().map(|t| t.0)
			.map(|i| named_tokens[&i].clone())
			.collect();

		Ok(tokens)
	}
}

#[cfg(test)]
mod tests {

	use tiny_keccak::Keccak;
	use crate::{
		token::Token, H256,
		Event, Param, ParamType,
	};
	use hex::FromHex;

	use std::prelude::v1::*;

	fn keccak256(data: &str) -> H256 {
		let mut result = [0u8; 32];
		let mut sponge = Keccak::new_keccak256();
		sponge.update(data.as_ref());
		sponge.finalize(&mut result);
		result.into()
	}

	#[test]
	fn test_decoding_event() {
		let mut event = Event {
			signature: "foo(int256,int256,address,address,string,int256[],address[5])",
			inputs: vec![
				Param { kind: ParamType::Int(256), indexed: false, },
				Param { kind: ParamType::Int(256), indexed: true, },
				Param { kind: ParamType::Address, indexed: false, },
				Param { kind: ParamType::Address, indexed: true, },
				Param { kind: ParamType::String, indexed: true, },
				Param {
					kind: ParamType::Array(Box::new(ParamType::Int(256))),
					indexed: true,
				},
				Param {
					kind: ParamType::FixedArray(Box::new(ParamType::Address), 5),
					indexed: true,
				},
			],
			anonymous: false,
		};

		let topics: Vec<H256> =  vec![
				keccak256("foo(int256,int256,address,address,string,int256[],address[5])"),
				"0000000000000000000000000000000000000000000000000000000000000002".parse().unwrap(),
				"0000000000000000000000001111111111111111111111111111111111111111".parse().unwrap(),
				"00000000000000000aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".parse().unwrap(),
				"00000000000000000bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".parse().unwrap(),
				"00000000000000000ccccccccccccccccccccccccccccccccccccccccccccccc".parse().unwrap(),
			];
		
		let data = concat!("0000000000000000000000000000000000000000000000000000000000000003",
						   "0000000000000000000000002222222222222222222222222222222222222222")
				.from_hex()
				.unwrap();
		
		let tokens = event.decode(topics, data).unwrap();

		assert_eq!(
			tokens,
			vec![
				Token::Int("0000000000000000000000000000000000000000000000000000000000000003".into()),
				Token::Int("0000000000000000000000000000000000000000000000000000000000000002".into()),
				Token::Address("2222222222222222222222222222222222222222".parse().unwrap()),
				Token::Address("1111111111111111111111111111111111111111".parse().unwrap()),
				Token::FixedBytes(
					"00000000000000000aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".from_hex().unwrap()
				),
				Token::FixedBytes(
					"00000000000000000bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".from_hex().unwrap()
				),
				Token::FixedBytes(
					"00000000000000000ccccccccccccccccccccccccccccccccccccccccccccccc".from_hex().unwrap()
				),
			]
		)
	}
}
