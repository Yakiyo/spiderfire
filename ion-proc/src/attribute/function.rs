/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/.
 */

use syn::{Error, Expr, Result};
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;

use crate::attribute::AttributeExt;

mod keywords {
	custom_keyword!(this);
	custom_keyword!(varargs);
	custom_keyword!(convert);
	custom_keyword!(strict);
}

struct ConvertArgument {
	_kw: keywords::convert,
	_eq: Token![=],
	pub(crate) conversion: Box<Expr>,
}

impl Parse for ConvertArgument {
	fn parse(input: ParseStream) -> Result<ConvertArgument> {
		let lookahead = input.lookahead1();
		if lookahead.peek(keywords::convert) {
			Ok(ConvertArgument {
				_kw: input.parse()?,
				_eq: input.parse()?,
				conversion: input.parse()?,
			})
		} else {
			Err(lookahead.error())
		}
	}
}

enum ParameterAttributeArgument {
	This(keywords::this),
	VarArgs(keywords::varargs),
	Convert(ConvertArgument),
	Strict(keywords::strict),
}

impl Parse for ParameterAttributeArgument {
	fn parse(input: ParseStream) -> Result<ParameterAttributeArgument> {
		use ParameterAttributeArgument as PAA;

		let lookahead = input.lookahead1();
		if lookahead.peek(keywords::this) {
			Ok(PAA::This(input.parse()?))
		} else if lookahead.peek(keywords::varargs) {
			Ok(PAA::VarArgs(input.parse()?))
		} else if lookahead.peek(keywords::convert) {
			Ok(PAA::Convert(input.parse()?))
		} else if lookahead.peek(keywords::strict) {
			Ok(PAA::Strict(input.parse()?))
		} else {
			Err(lookahead.error())
		}
	}
}

#[derive(Default)]
pub(crate) struct ParameterAttribute {
	pub(crate) this: bool,
	pub(crate) varargs: bool,
	pub(crate) convert: Option<Box<Expr>>,
	pub(crate) strict: bool,
}

impl Parse for ParameterAttribute {
	fn parse(input: ParseStream) -> Result<ParameterAttribute> {
		use ParameterAttributeArgument as PAA;
		let mut attributes = ParameterAttribute {
			this: false,
			varargs: false,
			convert: None,
			strict: false,
		};
		let span = input.span();

		let args = Punctuated::<PAA, Token![,]>::parse_terminated(input)?;
		for arg in args {
			match arg {
				PAA::This(_) => {
					if attributes.this {
						return Err(Error::new(span, "Parameter cannot have multiple `this` attributes."));
					}
					attributes.this = true
				}
				PAA::VarArgs(_) => {
					if attributes.varargs {
						return Err(Error::new(span, "Parameter cannot have multiple `varargs` attributes."));
					}
					attributes.varargs = true
				}
				PAA::Convert(ConvertArgument { conversion, .. }) => {
					if attributes.convert.is_some() {
						return Err(Error::new(span, "Parameter cannot have multiple `convert` attributes."));
					}
					attributes.convert = Some(conversion)
				}
				PAA::Strict(_) => attributes.strict = true,
			}
		}

		if attributes.this {
			if attributes.varargs || attributes.convert.is_some() || attributes.strict {
				return Err(Error::new(
					span,
					"Parameter with `this` attribute cannot have `varargs`, `convert`, or `strict` attributes.",
				));
			}
		}

		Ok(attributes)
	}
}

impl AttributeExt for ParameterAttribute {}
