use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default)]
pub struct Capabilities {
    pub headers_range: bool,
    pub blocks_by_height_bin: bool,
}

#[async_trait]
pub trait MoneroRpc: Send + Sync {
    async fn get_block_header_by_height(&self, height: u64)
        -> Result<GetBlockHeaderByHeightResult>;

    async fn get_block_headers_range(&self, start: u64, end: u64) -> Result<Vec<BlockHeader>>;

    async fn get_block(&self, hash: &str, fill_pow: bool) -> Result<GetBlockResult>;

    async fn get_transactions(&self, txs_hashes: &[String]) -> Result<GetTransactionsResult>;

    async fn get_block_count(&self) -> Result<GetBlockCountResult>;

    async fn get_transaction_pool_hashes(&self) -> Result<Vec<String>>;

    async fn probe_caps(&self) -> Capabilities;
}

#[derive(Clone)]
pub struct Rpc {
    base_json: String,
    base_rest: String,
    http: Client,
}

impl Rpc {
    pub fn new<S: Into<String>>(base: S) -> Self {
        let base_json = base.into();
        let base_rest = base_json
            .strip_suffix("/json_rpc")
            .unwrap_or(&base_json)
            .trim_end_matches('/')
            .to_string();

        Self {
            base_json,
            base_rest,
            http: Client::builder().build().expect("reqwest client"),
        }
    }

    async fn raw_call<T: for<'de> Deserialize<'de>, P: Serialize>(
        &self,
        method: &str,
        params: P,
    ) -> Result<T> {
        #[derive(Serialize)]
        struct Req<'a, P> {
            jsonrpc: &'a str,
            id: u64,
            method: &'a str,
            params: P,
        }

        #[derive(Deserialize)]
        struct RpcError {
            code: i64,
            message: String,
        }

        #[derive(Deserialize)]
        #[serde(untagged)]
        enum RpcResponse<T> {
            Ok { result: T },
            Err { error: RpcError },
        }

        let body = Req {
            jsonrpc: "2.0",
            id: 1,
            method,
            params,
        };

        let res = self
            .http
            .post(&self.base_json)
            .json(&body)
            .send()
            .await
            .with_context(|| format!("RPC {} send failed", method))?;

        let status = res.status();
        let v = res
            .json::<serde_json::Value>()
            .await
            .with_context(|| "RPC JSON decode failed".to_string())?;

        if !status.is_success() {
            anyhow::bail!("RPC {} HTTP {}: {}", method, status, v);
        }

        match serde_json::from_value::<RpcResponse<T>>(v)
            .with_context(|| "RPC result decode failed")?
        {
            RpcResponse::Ok { result } => Ok(result),
            RpcResponse::Err { error } => Err(anyhow!(
                "RPC {} error {}: {}",
                method,
                error.code,
                error.message
            )),
        }
    }

    async fn call<T: for<'de> Deserialize<'de>, P: Serialize>(
        &self,
        method: &str,
        params: P,
    ) -> Result<T> {
        self.raw_call(method, params).await
    }

    pub async fn probe_caps(&self) -> Capabilities {
        let hdrs_ok = self
            .raw_call::<serde_json::Value, _>(
                "get_block_headers_range",
                serde_json::json!({
                    "start_height": 0,
                    "end_height": 0,
                }),
            )
            .await
            .is_ok();

        let bin_url = format!("{}/get_blocks_by_height.bin?heights=0", self.base_rest);

        let bin_ok = match self.http.head(&bin_url).send().await {
            Ok(res) if res.status().is_success() => true,
            _ => match self.http.get(&bin_url).send().await {
                Ok(res) => res.status().is_success(),
                Err(_) => false,
            },
        };

        Capabilities {
            headers_range: hdrs_ok,
            blocks_by_height_bin: bin_ok,
        }
    }

    pub async fn get_block_header_by_height(
        &self,
        height: u64,
    ) -> Result<GetBlockHeaderByHeightResult> {
        #[derive(Serialize)]
        struct P {
            height: u64,
        }

        self.call("get_block_header_by_height", P { height }).await
    }

    pub async fn get_block_headers_range(&self, start: u64, end: u64) -> Result<Vec<BlockHeader>> {
        #[derive(Serialize)]
        struct P {
            start_height: u64,
            end_height: u64,
        }

        #[derive(Deserialize)]
        struct R {
            status: String,
            headers: Vec<BlockHeader>,
        }

        let r: R = self
            .raw_call(
                "get_block_headers_range",
                &P {
                    start_height: start,
                    end_height: end,
                },
            )
            .await?;
        anyhow::ensure!(r.status == "OK", "bad status");
        Ok(r.headers)
    }

    pub async fn get_block(&self, hash: &str, fill_pow: bool) -> Result<GetBlockResult> {
        #[derive(Serialize)]
        struct P<'a> {
            hash: &'a str,
            fill_pow: bool,
        }

        self.call("get_block", P { hash, fill_pow }).await
    }

    pub async fn get_transactions(&self, txs_hashes: &[String]) -> Result<GetTransactionsResult> {
        #[derive(Serialize)]
        struct P<'a> {
            txs_hashes: &'a [String],
            decode_as_json: bool,
            prune: bool,
        }

        let url = format!("{}/get_transactions", self.base_rest);
        let res = self
            .http
            .post(&url)
            .json(&P {
                txs_hashes,
                decode_as_json: true,
                prune: false,
            })
            .send()
            .await
            .with_context(|| "get_transactions send failed".to_string())?;

        let status = res.status();
        if !status.is_success() {
            let body = res
                .text()
                .await
                .unwrap_or_else(|_| "<binary response>".to_string());
            anyhow::bail!("get_transactions HTTP {}: {}", status, body);
        }

        res.json::<GetTransactionsResult>()
            .await
            .with_context(|| "get_transactions decode failed".to_string())
    }

    pub async fn get_block_count(&self) -> Result<GetBlockCountResult> {
        self.call("get_block_count", ()).await
    }

    pub async fn get_transaction_pool_hashes(&self) -> Result<Vec<String>> {
        #[derive(Deserialize)]
        struct RestResponse {
            status: String,
            #[serde(default)]
            tx_hashes: Vec<String>,
        }

        let url = format!("{}/get_transaction_pool_hashes", self.base_rest);
        let res = self
            .http
            .get(&url)
            .send()
            .await
            .with_context(|| "get_transaction_pool_hashes send failed".to_string())?;

        let status = res.status();
        let body = res
            .json::<RestResponse>()
            .await
            .with_context(|| "get_transaction_pool_hashes decode failed".to_string())?;

        if !status.is_success() {
            anyhow::bail!(
                "get_transaction_pool_hashes HTTP {} status {}",
                status,
                body.status
            );
        }

        if body.status == "OK" {
            Ok(body.tx_hashes)
        } else {
            Err(anyhow!(
                "get_transaction_pool_hashes status {}",
                body.status
            ))
        }
    }
}

#[async_trait]
impl MoneroRpc for Rpc {
    async fn get_block_headers_range(&self, start: u64, end: u64) -> Result<Vec<BlockHeader>> {
        Rpc::get_block_headers_range(self, start, end).await
    }

    async fn get_block_header_by_height(
        &self,
        height: u64,
    ) -> Result<GetBlockHeaderByHeightResult> {
        Rpc::get_block_header_by_height(self, height).await
    }

    async fn get_block(&self, hash: &str, fill_pow: bool) -> Result<GetBlockResult> {
        Rpc::get_block(self, hash, fill_pow).await
    }

    async fn get_transactions(&self, txs_hashes: &[String]) -> Result<GetTransactionsResult> {
        Rpc::get_transactions(self, txs_hashes).await
    }

    async fn get_block_count(&self) -> Result<GetBlockCountResult> {
        Rpc::get_block_count(self).await
    }

    async fn get_transaction_pool_hashes(&self) -> Result<Vec<String>> {
        Rpc::get_transaction_pool_hashes(self).await
    }

    async fn probe_caps(&self) -> Capabilities {
        Rpc::probe_caps(self).await
    }
}

#[derive(Debug, Deserialize)]
pub struct GetBlockHeaderByHeightResult {
    pub block_header: BlockHeader,
    pub status: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct BlockHeader {
    pub hash: String,
    pub height: u64,
    pub timestamp: u64,
    pub prev_hash: String,
    pub major_version: u32,
    pub minor_version: u32,
    pub nonce: u64,
    pub reward: u64,
    #[serde(default, alias = "block_size")]
    pub size: u64,
}

#[derive(Debug, Deserialize)]
pub struct GetBlockResult {
    pub block_header: BlockHeader,
    #[serde(default)]
    pub json: Option<String>,
    #[serde(default)]
    pub blob: Option<String>,
    #[serde(default)]
    pub miner_tx_hash: Option<String>,
    pub status: String,
}

#[derive(Debug, Deserialize)]
pub struct GetTransactionsResult {
    #[serde(default)]
    pub txs_as_json: Vec<String>,
    #[serde(default)]
    pub missed_tx: Vec<String>,
    pub status: String,
}

#[derive(Debug, Deserialize)]
pub struct GetBlockCountResult {
    pub count: u64,
    pub status: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use httpmock::{prelude::*, Method::HEAD};
    use serde_json::json;

    #[tokio::test]
    async fn transaction_pool_hashes_success() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET).path("/get_transaction_pool_hashes");
            then.status(200).json_body(json!({
                "status": "OK",
                "tx_hashes": ["abcdef"],
            }));
        });

        let rpc = Rpc::new(format!("{}/json_rpc", server.url("")));
        let hashes = rpc
            .get_transaction_pool_hashes()
            .await
            .expect("pool hashes success");

        assert_eq!(hashes, vec!["abcdef".to_string()]);
        mock.assert();
    }

    #[tokio::test]
    async fn transaction_pool_hashes_non_ok_status() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET).path("/get_transaction_pool_hashes");
            then.status(200).json_body(json!({
                "status": "BUSY",
                "tx_hashes": ["abcdef"],
            }));
        });

        let rpc = Rpc::new(format!("{}/json_rpc", server.url("")));
        let err = rpc.get_transaction_pool_hashes().await.unwrap_err();

        assert!(err
            .to_string()
            .contains("get_transaction_pool_hashes status BUSY"));
        mock.assert();
    }

    #[tokio::test]
    async fn get_transactions_via_rest() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/get_transactions")
                .json_body(json!({
                    "txs_hashes": ["deadbeef"],
                    "decode_as_json": true,
                    "prune": false,
                }));
            then.status(200).json_body(json!({
                "status": "OK",
                "txs_as_json": ["{\"tx_hash\":\"deadbeef\"}"],
                "missed_tx": [],
            }));
        });

        let rpc = Rpc::new(format!("{}/json_rpc", server.url("")));
        let hashes = vec!["deadbeef".to_string()];
        let res = rpc
            .get_transactions(&hashes)
            .await
            .expect("rest get_transactions");

        assert_eq!(
            res.txs_as_json,
            vec!["{\"tx_hash\":\"deadbeef\"}".to_string()]
        );
        assert!(res.missed_tx.is_empty());
        mock.assert();
    }

    #[tokio::test]
    async fn probe_caps_detects_range_and_bin() {
        let server = MockServer::start();
        let rpc = Rpc::new(format!("{}/json_rpc", server.url("")));

        let _range = server.mock(|when, then| {
            when.method(POST).path("/json_rpc").json_body(json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "get_block_headers_range",
                "params": {"start_height": 0, "end_height": 0},
            }));
            then.status(200).json_body(json!({
                "jsonrpc": "2.0",
                "id": 1,
                "result": {"status": "OK", "headers": []},
            }));
        });

        let _bin = server.mock(|when, then| {
            when.method(HEAD)
                .path("/get_blocks_by_height.bin")
                .query_param("heights", "0");
            then.status(200);
        });

        let caps = rpc.probe_caps().await;
        assert!(caps.headers_range);
        assert!(caps.blocks_by_height_bin);
    }
}
