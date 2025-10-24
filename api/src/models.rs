use serde::Serialize;

#[derive(Serialize, sqlx::FromRow)]
pub struct BlockView {
    pub height: i64,
    pub hash: Option<String>,
    pub ts: Option<i64>,
    pub size_bytes: i32,
    pub major_version: i32,
    pub minor_version: i32,
    pub tx_count: i32,
    pub reward_nanos: i64,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct TxView {
    pub hash: Option<String>,
    pub block_height: Option<i64>,
    pub ts: Option<i64>,
    pub in_mempool: bool,
    pub fee_nanos: Option<i64>,
    pub size_bytes: i32,
    pub version: i32,
    pub unlock_time: i64,
    pub extra_json: Option<String>,
    pub rct_type: i32,
    pub proof_type: Option<String>,
    pub bp_plus: bool,
    pub num_inputs: i32,
    pub num_outputs: i32,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct RingView {
    pub tx_hash: Option<String>,
    pub input_idx: i32,
    pub ring_index: i32,
    pub global_index: Option<i64>,
}

#[derive(Serialize)]
pub struct RingMemberView {
    pub ring_index: i32,
    pub global_index: Option<i64>,
}

#[derive(Serialize)]
pub struct RingSetView {
    pub input_idx: i32,
    pub members: Vec<RingMemberView>,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct KeyImageView {
    pub key_image: Option<String>,
    pub spending_tx: Option<String>,
    pub block_height: Option<i64>,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct MempoolView {
    pub hash: Option<String>,
    pub first_seen: Option<i64>,
    pub last_seen: Option<i64>,
    pub fee_rate: Option<rust_decimal::Decimal>,
    pub relayed_by: Option<String>,
}

#[derive(Serialize)]
pub struct SearchResult {
    pub kind: String,
    pub value: serde_json::Value,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct InputView {
    pub idx: i32,
    pub key_image: String,
    pub ring_size: i32,
    pub pseudo_out: Option<String>,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct OutputView {
    pub idx_in_tx: i32,
    pub global_index: Option<i64>,
    pub amount: Option<rust_decimal::Decimal>,
    pub commitment: String,
    pub stealth_public_key: String,
    pub spent_by_key_image: Option<String>,
    pub spent_in_tx: Option<String>,
}

#[derive(Serialize)]
pub struct TxDetailView {
    #[serde(flatten)]
    pub tx: TxView,
    pub inputs: Vec<InputView>,
    pub outputs: Vec<OutputView>,
}
