use anyhow::{anyhow, Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Clone)]
pub struct Rpc {
    base: String,
    http: Client,
}

impl Rpc {
    pub fn new<S: Into<String>>(base: S) -> Self {
        Self {
            base: base.into(),
            http: Client::builder().build().expect("reqwest client"),
        }
    }

    async fn call<T: for<'de> Deserialize<'de>, P: Serialize>(
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
        struct Res<T> {
            result: T,
        }

        let body = Req {
            jsonrpc: "2.0",
            id: 1,
            method,
            params,
        };

        let res = self
            .http
            .post(&self.base)
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

        let result: Res<T> =
            serde_json::from_value(v).with_context(|| "RPC result decode failed")?;
        Ok(result.result)
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

        self.call(
            "get_transactions",
            P {
                txs_hashes,
                decode_as_json: true,
                prune: false,
            },
        )
        .await
    }

    pub async fn get_block_count(&self) -> Result<GetBlockCountResult> {
        self.call("get_block_count", ()).await
    }

    pub async fn get_transaction_pool_hashes(&self) -> Result<Vec<String>> {
        #[derive(Deserialize)]
        struct R {
            status: String,
            #[serde(default)]
            tx_hashes: Vec<String>,
        }

        let res: R = self
            .call("get_transaction_pool_hashes", json!({}))
            .await
            .context("get_transaction_pool_hashes rpc")?;

        if res.status == "OK" {
            Ok(res.tx_hashes)
        } else {
            Err(anyhow!("get_transaction_pool_hashes status {}", res.status))
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct GetBlockHeaderByHeightResult {
    pub block_header: BlockHeader,
    pub status: String,
}

#[derive(Debug, Deserialize)]
pub struct BlockHeader {
    pub hash: String,
    pub height: u64,
    pub timestamp: u64,
    pub prev_hash: String,
    pub major_version: u32,
    pub minor_version: u32,
    pub nonce: u64,
    pub reward: u64,
    pub size: u64,
}

#[derive(Debug, Deserialize)]
pub struct GetBlockResult {
    pub block_header: BlockHeader,
    #[serde(default)]
    pub json: Option<String>,
    #[serde(default)]
    pub blob: Option<String>,
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
    use httpmock::prelude::*;

    #[tokio::test]
    async fn transaction_pool_hashes_success() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST).path("/").json_body(json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "get_transaction_pool_hashes",
                "params": {}
            }));
            then.status(200).json_body(json!({
                "result": {
                    "status": "OK",
                    "tx_hashes": ["abcdef"],
                }
            }));
        });

        let rpc = Rpc::new(server.url("/"));
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
            when.method(POST).path("/").json_body(json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "get_transaction_pool_hashes",
                "params": {}
            }));
            then.status(200).json_body(json!({
                "result": {
                    "status": "BUSY",
                    "tx_hashes": ["abcdef"],
                }
            }));
        });

        let rpc = Rpc::new(server.url("/"));
        let err = rpc.get_transaction_pool_hashes().await.unwrap_err();

        assert!(err
            .to_string()
            .contains("get_transaction_pool_hashes status BUSY"));
        mock.assert();
    }
}
