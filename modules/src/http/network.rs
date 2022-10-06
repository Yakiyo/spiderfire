/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/.
 */

use std::str::FromStr;

use bytes::Bytes;
use http::{Method, StatusCode, Uri};
use http::header::{CONTENT_ENCODING, CONTENT_LANGUAGE, CONTENT_LOCATION, CONTENT_TYPE, HOST, LOCATION};
use hyper::Body;
use url::Url;

use ion::{Error, Result};

use crate::http::{Request, Response};
use crate::http::request::{add_host_header, clone_request, Redirection};

pub(crate) async fn request_internal(mut req: Request) -> Result<Response> {
	let client = req.client.to_client();
	let mut redirections = 0;

	let mut request = req.clone()?;
	*request.request.body_mut() = Body::from(request.body.clone());

	*req.request.body_mut() = Body::from(req.body);
	let mut response = client.request(req.request).await?;
	let mut locations = vec![request.url.clone()];

	while response.status().is_redirection() {
		if redirections >= 20 {
			return Err(Error::new("Too Many Redirects", None));
		}
		let status = response.status();
		if status != StatusCode::SEE_OTHER && !request.body.is_empty() {
			return Err(Error::new("Redirected with a Body", None));
		}

		match req.redirection {
			Redirection::Follow => {
				let method = request.request.method().clone();

				if let Some(location) = response.headers().get(LOCATION) {
					let location = location.to_str()?;
					let url = {
						let options = Url::options();
						options.base_url(Some(&request.url));
						options.parse(location)
					}?;

					redirections += 1;

					if ((status == StatusCode::MOVED_PERMANENTLY || status == StatusCode::FOUND) && method == Method::POST)
						|| (status == StatusCode::SEE_OTHER && (method != Method::GET && method != Method::HEAD))
					{
						*request.request.method_mut() = Method::GET;

						request.body = Bytes::new();
						*request.request.body_mut() = Body::empty();

						let headers = request.request.headers_mut();
						headers.remove(CONTENT_ENCODING);
						headers.remove(CONTENT_LANGUAGE);
						headers.remove(CONTENT_LOCATION);
						headers.remove(CONTENT_TYPE);
					}

					request.request.headers_mut().remove(HOST);
					add_host_header(request.request.headers_mut(), &url, true)?;

					locations.push(url.clone());
					*request.request.uri_mut() = Uri::from_str(url.as_str())?;

					let request = { clone_request(&request.request) }?;
					response = client.request(request).await?;
				} else {
					return Ok(Response::new(response, redirections, locations));
				}
			}
			Redirection::Error => return Err(Error::new("Received Redirection", None)),
			Redirection::Manual => return Ok(Response::new(response, redirections, locations)),
		}
	}

	Ok(Response::new(response, redirections, locations))
}
