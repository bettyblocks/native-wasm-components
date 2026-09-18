use wstd::http::{Body, Client, Request};

pub(crate) trait HttpClient {
    async fn post_json(
        &self,
        url: &str,
        headers: Vec<(&'static str, String)>,
        body: Vec<u8>,
    ) -> Result<(u16, Vec<u8>), String>;
}

impl HttpClient for Client {
    async fn post_json(
        &self,
        url: &str,
        headers: Vec<(&'static str, String)>,
        body: Vec<u8>,
    ) -> Result<(u16, Vec<u8>), String> {
        let mut builder = Request::post(url);

        for (name, value) in headers {
            builder = builder.header(name, value);
        }

        let request = builder.body(Body::from(body)).map_err(|e| e.to_string())?;
        let response = self.send(request).await.map_err(|e| e.to_string())?;

        let status = response.status().as_u16();
        let (_, mut body) = response.into_parts();
        let bytes = body.contents().await.map_err(|e| e.to_string())?.to_vec();

        Ok((status, bytes))
    }
}
