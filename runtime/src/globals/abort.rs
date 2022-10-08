/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/.
 */

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::task::Poll;

use futures::FutureExt;
use mozjs::jsval::JSVal;
use tokio::sync::watch::Receiver;

pub use controller::AbortController;
use ion::{ClassInitialiser, Context, Object};
pub use signal::AbortSignal;

#[derive(Clone, Debug)]
pub enum Signal {
	None,
	Abort(JSVal),
	Receiver(Receiver<Option<JSVal>>),
	Timeout(Receiver<Option<JSVal>>, Arc<AtomicBool>),
}

impl Default for Signal {
	fn default() -> Self {
		Signal::None
	}
}

pub struct SignalFuture {
	inner: Signal,
}

impl Future for SignalFuture {
	type Output = JSVal;

	fn poll(mut self: Pin<&mut SignalFuture>, cx: &mut std::task::Context<'_>) -> Poll<JSVal> {
		match &mut self.inner {
			Signal::None => Poll::Pending,
			Signal::Abort(abort) => Poll::Ready(*abort),
			Signal::Receiver(receiver) | Signal::Timeout(receiver, _) => {
				if let Some(abort) = *receiver.borrow() {
					return Poll::Ready(abort);
				}
				let changed = { Box::pin(receiver.changed()).poll_unpin(cx) };
				match changed {
					Poll::Ready(_) => match *receiver.borrow() {
						Some(abort) => Poll::Ready(abort),
						None => {
							cx.waker().wake_by_ref();
							Poll::Pending
						}
					},
					Poll::Pending => {
						cx.waker().wake_by_ref();
						Poll::Pending
					}
				}
			}
		}
	}
}

impl Drop for SignalFuture {
	fn drop(&mut self) {
		if let Signal::Timeout(receiver, terminate) = &self.inner {
			if receiver.borrow().is_none() {
				terminate.store(true, Ordering::SeqCst);
			}
		}
	}
}

#[js_class]
mod controller {
	use mozjs::conversions::ToJSValConvertible;
	use mozjs::jsval::{JSVal, NullValue};
	use tokio::sync::watch::{channel, Sender};

	use ion::Error;

	use crate::globals::abort::{AbortSignal, Signal};

	pub struct AbortController {
		sender: Sender<Option<JSVal>>,
	}

	impl AbortController {
		#[ion(constructor)]
		pub fn constructor() -> AbortController {
			let (sender, _) = channel(None);
			AbortController { sender }
		}

		#[ion(get)]
		pub fn get_signal(&self) -> AbortSignal {
			AbortSignal {
				signal: Signal::Receiver(self.sender.subscribe()),
			}
		}

		pub fn abort(&self, cx: Context, reason: Option<JSVal>) {
			let none = reason.is_none();
			rooted!(in(cx) let mut reason = reason.unwrap_or_else(NullValue));
			if none {
				unsafe {
					Error::new("AbortError", None).to_jsval(cx, reason.handle_mut());
				}
			}
			self.sender.send_replace(Some(reason.get()));
		}
	}
}

#[js_class]
mod signal {
	use std::result;
	use std::sync::Arc;
	use std::sync::atomic::AtomicBool;

	use chrono::Duration;
	use mozjs::conversions::{ConversionBehavior, ConversionResult, FromJSValConvertible, ToJSValConvertible};
	use mozjs::jsval::{JSVal, NullValue};
	use mozjs::rust::HandleValue;
	use tokio::sync::watch::channel;

	use ion::{Context, Error, Exception, Result};
	use ion::class::class_from_jsval;

	use crate::event_loop::EVENT_LOOP;
	use crate::event_loop::macrotasks::{Macrotask, SignalMacrotask};
	use crate::globals::abort::{Signal, SignalFuture};

	#[derive(Clone, Default)]
	pub struct AbortSignal {
		pub(crate) signal: Signal,
	}

	impl AbortSignal {
		#[ion(constructor)]
		pub fn constructor() -> Result<AbortSignal> {
			Err(Error::new("Constructor should not be called.", None))
		}

		#[ion(internal)]
		pub fn poll(&self) -> SignalFuture {
			SignalFuture { inner: self.signal.clone() }
		}

		#[ion(get)]
		pub fn get_aborted(&self) -> bool {
			self.get_reason().is_some()
		}

		#[ion(get)]
		pub fn get_reason(&self) -> Option<JSVal> {
			match &self.signal {
				Signal::None => None,
				Signal::Abort(abort) => Some(*abort),
				Signal::Receiver(receiver) | Signal::Timeout(receiver, _) => *receiver.borrow(),
			}
		}

		pub fn throwIfAborted(&self) -> result::Result<(), Exception> {
			if let Some(reason) = self.get_reason() {
				Err(Exception::Other(reason))
			} else {
				Ok(())
			}
		}

		pub fn abort(cx: Context, reason: Option<JSVal>) -> AbortSignal {
			let none = reason.is_none();
			rooted!(in(cx) let mut reason = reason.unwrap_or_else(NullValue));
			if none {
				unsafe {
					Error::new("AbortError", None).to_jsval(cx, reason.handle_mut());
				}
			}
			AbortSignal { signal: Signal::Abort(reason.get()) }
		}

		pub fn timeout(cx: Context, #[ion(convert = ConversionBehavior::EnforceRange)] time: u64) -> AbortSignal {
			let (sender, receiver) = channel(None);
			let terminate = Arc::new(AtomicBool::new(false));
			let terminate2 = terminate.clone();

			let callback = Box::new(move || {
				rooted!(in(cx) let mut error = NullValue());
				unsafe {
					Error::new(&format!("Timeout Error: {}ms", time), None).to_jsval(cx, error.handle_mut());
				}
				sender.send_replace(Some(error.get()));
			});

			let duration = Duration::milliseconds(time as i64);
			EVENT_LOOP.with(|event_loop| {
				if let Some(queue) = (*event_loop.borrow_mut()).macrotasks.as_mut() {
					queue.enqueue(Macrotask::Signal(SignalMacrotask::new(callback, terminate, duration)), None);
				}
			});
			AbortSignal {
				signal: Signal::Timeout(receiver, terminate2),
			}
		}
	}

	impl FromJSValConvertible for AbortSignal {
		type Config = ();

		unsafe fn from_jsval(cx: Context, val: HandleValue, _: ()) -> result::Result<ConversionResult<AbortSignal>, ()> {
			class_from_jsval(cx, val)
		}
	}
}

pub fn define(cx: Context, global: Object) -> bool {
	AbortController::init_class(cx, &global);
	AbortSignal::init_class(cx, &global);
	true
}