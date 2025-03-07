// Copyright 2023 Greptime Team
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::collections::HashSet;
use std::sync::Arc;

use api::v1::meta::store_client::StoreClient;
use api::v1::meta::{
    BatchDeleteRequest, BatchDeleteResponse, BatchGetRequest, BatchGetResponse, BatchPutRequest,
    BatchPutResponse, CompareAndPutRequest, CompareAndPutResponse, DeleteRangeRequest,
    DeleteRangeResponse, PutRequest, PutResponse, RangeRequest, RangeResponse, Role,
};
use common_grpc::channel_manager::ChannelManager;
use common_telemetry::tracing_context::TracingContext;
use snafu::{ensure, OptionExt, ResultExt};
use tokio::sync::RwLock;
use tonic::codec::CompressionEncoding;
use tonic::transport::Channel;

use crate::client::{load_balance as lb, Id};
use crate::error;
use crate::error::Result;

#[derive(Clone, Debug)]
pub struct Client {
    inner: Arc<RwLock<Inner>>,
}

impl Client {
    pub fn new(id: Id, role: Role, channel_manager: ChannelManager) -> Self {
        let inner = Arc::new(RwLock::new(Inner {
            id,
            role,
            channel_manager,
            peers: vec![],
        }));

        Self { inner }
    }

    pub async fn start<U, A>(&mut self, urls: A) -> Result<()>
    where
        U: AsRef<str>,
        A: AsRef<[U]>,
    {
        let mut inner = self.inner.write().await;
        inner.start(urls).await
    }

    pub async fn range(&self, req: RangeRequest) -> Result<RangeResponse> {
        let inner = self.inner.read().await;
        inner.range(req).await
    }

    pub async fn put(&self, req: PutRequest) -> Result<PutResponse> {
        let inner = self.inner.read().await;
        inner.put(req).await
    }

    pub async fn batch_get(&self, req: BatchGetRequest) -> Result<BatchGetResponse> {
        let inner = self.inner.read().await;
        inner.batch_get(req).await
    }

    pub async fn batch_put(&self, req: BatchPutRequest) -> Result<BatchPutResponse> {
        let inner = self.inner.read().await;
        inner.batch_put(req).await
    }

    pub async fn batch_delete(&self, req: BatchDeleteRequest) -> Result<BatchDeleteResponse> {
        let inner = self.inner.read().await;
        inner.batch_delete(req).await
    }

    pub async fn compare_and_put(
        &self,
        req: CompareAndPutRequest,
    ) -> Result<CompareAndPutResponse> {
        let inner = self.inner.read().await;
        inner.compare_and_put(req).await
    }

    pub async fn delete_range(&self, req: DeleteRangeRequest) -> Result<DeleteRangeResponse> {
        let inner = self.inner.read().await;
        inner.delete_range(req).await
    }
}

#[derive(Debug)]
struct Inner {
    id: Id,
    role: Role,
    channel_manager: ChannelManager,
    peers: Vec<String>,
}

impl Inner {
    async fn start<U, A>(&mut self, urls: A) -> Result<()>
    where
        U: AsRef<str>,
        A: AsRef<[U]>,
    {
        ensure!(
            !self.is_started(),
            error::IllegalGrpcClientStateSnafu {
                err_msg: "Store client already started",
            }
        );

        self.peers = urls
            .as_ref()
            .iter()
            .map(|url| url.as_ref().to_string())
            .collect::<HashSet<_>>()
            .drain()
            .collect::<Vec<_>>();

        Ok(())
    }

    async fn range(&self, mut req: RangeRequest) -> Result<RangeResponse> {
        let mut client = self.random_client()?;
        req.set_header(
            self.id,
            self.role,
            TracingContext::from_current_span().to_w3c(),
        );
        let res = client.range(req).await.map_err(error::Error::from)?;

        Ok(res.into_inner())
    }

    async fn put(&self, mut req: PutRequest) -> Result<PutResponse> {
        let mut client = self.random_client()?;
        req.set_header(
            self.id,
            self.role,
            TracingContext::from_current_span().to_w3c(),
        );
        let res = client.put(req).await.map_err(error::Error::from)?;

        Ok(res.into_inner())
    }

    async fn batch_get(&self, mut req: BatchGetRequest) -> Result<BatchGetResponse> {
        let mut client = self.random_client()?;
        req.set_header(
            self.id,
            self.role,
            TracingContext::from_current_span().to_w3c(),
        );

        let res = client.batch_get(req).await.map_err(error::Error::from)?;

        Ok(res.into_inner())
    }

    async fn batch_put(&self, mut req: BatchPutRequest) -> Result<BatchPutResponse> {
        let mut client = self.random_client()?;
        req.set_header(
            self.id,
            self.role,
            TracingContext::from_current_span().to_w3c(),
        );
        let res = client.batch_put(req).await.map_err(error::Error::from)?;

        Ok(res.into_inner())
    }

    async fn batch_delete(&self, mut req: BatchDeleteRequest) -> Result<BatchDeleteResponse> {
        let mut client = self.random_client()?;
        req.set_header(
            self.id,
            self.role,
            TracingContext::from_current_span().to_w3c(),
        );
        let res = client.batch_delete(req).await.map_err(error::Error::from)?;

        Ok(res.into_inner())
    }

    async fn compare_and_put(
        &self,
        mut req: CompareAndPutRequest,
    ) -> Result<CompareAndPutResponse> {
        let mut client = self.random_client()?;
        req.set_header(
            self.id,
            self.role,
            TracingContext::from_current_span().to_w3c(),
        );
        let res = client
            .compare_and_put(req)
            .await
            .map_err(error::Error::from)?;

        Ok(res.into_inner())
    }

    async fn delete_range(&self, mut req: DeleteRangeRequest) -> Result<DeleteRangeResponse> {
        let mut client = self.random_client()?;
        req.set_header(
            self.id,
            self.role,
            TracingContext::from_current_span().to_w3c(),
        );
        let res = client.delete_range(req).await.map_err(error::Error::from)?;

        Ok(res.into_inner())
    }

    fn random_client(&self) -> Result<StoreClient<Channel>> {
        let len = self.peers.len();
        let peer = lb::random_get(len, |i| Some(&self.peers[i])).context(
            error::IllegalGrpcClientStateSnafu {
                err_msg: "Empty peers, store client may not start yet",
            },
        )?;

        self.make_client(peer)
    }

    fn make_client(&self, addr: impl AsRef<str>) -> Result<StoreClient<Channel>> {
        let channel = self
            .channel_manager
            .get(addr)
            .context(error::CreateChannelSnafu)?;

        Ok(StoreClient::new(channel)
            .accept_compressed(CompressionEncoding::Gzip)
            .accept_compressed(CompressionEncoding::Zstd)
            .send_compressed(CompressionEncoding::Zstd))
    }

    #[inline]
    fn is_started(&self) -> bool {
        !self.peers.is_empty()
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[tokio::test]
    async fn test_already_start() {
        let mut client = Client::new(0, Role::Frontend, ChannelManager::default());
        client
            .start(&["127.0.0.1:1000", "127.0.0.1:1001"])
            .await
            .unwrap();
        let res = client.start(&["127.0.0.1:1002"]).await;
        assert!(res.is_err());
        assert!(matches!(
            res.err(),
            Some(error::Error::IllegalGrpcClientState { .. })
        ));
    }

    #[tokio::test]
    async fn test_start_with_duplicate_peers() {
        let mut client = Client::new(0, Role::Frontend, ChannelManager::default());
        client
            .start(&["127.0.0.1:1000", "127.0.0.1:1000", "127.0.0.1:1000"])
            .await
            .unwrap();
        assert_eq!(1, client.inner.write().await.peers.len());
    }
}
