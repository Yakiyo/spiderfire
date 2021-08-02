/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/.
 */

use std::result::Result;

use chrono::DateTime;
use chrono::offset::Utc;
use mozjs::conversions::{ConversionResult, FromJSValConvertible, ToJSValConvertible};
use mozjs::error::throw_type_error;
use mozjs::jsapi::{AssertSameCompartment, ClippedTime, DateIsValid, NewDateObject, ObjectIsDate, Value};
use mozjs::jsval::ObjectValue;
use mozjs::rust::{HandleValue, maybe_wrap_object_value, MutableHandleValue};

use crate::functions::macros::IonContext;
use crate::objects::object::IonRawObject;

pub struct IonDate {
	obj: IonRawObject,
}

impl IonDate {
	pub unsafe fn new(cx: IonContext) -> IonDate {
		IonDate::from_date(cx, Utc::now())
	}

	pub unsafe fn from_date(cx: IonContext, time: DateTime<Utc>) -> IonDate {
		IonDate::from(
			cx,
			NewDateObject(
				cx,
				ClippedTime {
					t: time.timestamp_millis() as f64,
				},
			),
		)
		.unwrap()
	}

	pub unsafe fn from(cx: IonContext, obj: IonRawObject) -> Option<IonDate> {
		if IonDate::is_date_raw(cx, obj) {
			Some(IonDate { obj })
		} else {
			throw_type_error(cx, "Object cannot be converted to Date");
			None
		}
	}

	pub unsafe fn from_value(cx: IonContext, val: Value) -> Option<IonDate> {
		assert!(val.is_object());
		IonDate::from(cx, val.to_object())
	}

	pub unsafe fn raw(&self) -> IonRawObject {
		self.obj
	}

	pub unsafe fn is_valid(&self, cx: IonContext) -> bool {
		rooted!(in(cx) let obj = self.obj);
		let mut is_valid = true;
		return DateIsValid(cx, obj.handle().into(), &mut is_valid) && is_valid;
	}

	pub unsafe fn is_date_raw(cx: IonContext, obj: IonRawObject) -> bool {
		rooted!(in(cx) let mut robj = obj);
		let mut is_date = false;
		ObjectIsDate(cx, robj.handle_mut().into(), &mut is_date) && is_date
	}

	pub unsafe fn is_date(&self, cx: IonContext) -> bool {
		IonDate::is_date_raw(cx, self.obj)
	}
}

impl FromJSValConvertible for IonDate {
	type Config = ();
	#[inline]
	unsafe fn from_jsval(cx: IonContext, value: HandleValue, _option: ()) -> Result<ConversionResult<IonDate>, ()> {
		if !value.is_object() {
			throw_type_error(cx, "Value is not an object");
			return Err(());
		}

		AssertSameCompartment(cx, value.to_object());
		if let Some(date) = IonDate::from(cx, value.to_object()) {
			Ok(ConversionResult::Success(date))
		} else {
			Err(())
		}
	}
}

impl ToJSValConvertible for IonDate {
	#[inline]
	unsafe fn to_jsval(&self, cx: IonContext, mut rval: MutableHandleValue) {
		rval.set(ObjectValue(self.raw()));
		maybe_wrap_object_value(cx, rval);
	}
}