use wstd::http::{Body, Client, Method, Request};

pub(crate) trait HttpClient {
    async fn send_request(
        &self,
        method: Method,
        url: &str,
        headers: Vec<(String, String)>,
        body: Vec<u8>,
    ) -> Result<(u16, Vec<u8>), String>;
}

impl HttpClient for Client {
    async fn send_request(
        &self,
        method: Method,
        url: &str,
        headers: Vec<(String, String)>,
        body: Vec<u8>,
    ) -> Result<(u16, Vec<u8>), String> {
        let request = headers
            .into_iter()
            .fold(
                Request::builder().method(method).uri(url),
                |builder, (name, value)| builder.header(name, value),
            )
            .body(Body::from(body))
            .map_err(|error| error.to_string())?;

        let response = self.send(request).await.map_err(|error| error.to_string())?;
        let status = response.status().as_u16();
        let (_, mut response_body) = response.into_parts();
        let bytes = response_body
            .contents()
            .await
            .map_err(|error| error.to_string())?
            .to_vec();

        Ok((status, bytes))
    }
}
