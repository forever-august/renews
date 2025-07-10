//! Article submission queue and worker pool
//!
//! This module implements a queue-based article submission system using flume.
//! Articles are validated minimally on submission, queued, and then processed
//! by background workers that perform comprehensive validation and storage.

use crate::Message;
use crate::auth::DynAuth;
use crate::config::Config;
use crate::storage::DynStorage;
use flume::{Receiver, Sender};
use std::error::Error;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info};

/// An article queued for processing
#[derive(Debug, Clone)]
pub struct QueuedArticle {
    /// The parsed message
    pub message: Message,
    /// Size of the original message in bytes
    pub size: u64,
    /// Whether this is a control message
    pub is_control: bool,
    /// Whether comprehensive validation has already been done
    pub already_validated: bool,
}

/// Article processing queue using flume MPMC
#[derive(Clone)]
pub struct ArticleQueue {
    sender: Sender<QueuedArticle>,
    receiver: Receiver<QueuedArticle>,
}

impl ArticleQueue {
    /// Create a new article queue with the specified capacity
    pub fn new(capacity: usize) -> Self {
        let (sender, receiver) = flume::bounded(capacity);
        Self { sender, receiver }
    }

    /// Submit an article to the queue for processing
    ///
    /// Returns Ok(()) if the article was queued successfully,
    /// Err if the queue is full or closed.
    pub async fn submit(&self, article: QueuedArticle) -> Result<(), Box<dyn Error + Send + Sync>> {
        self.sender
            .send_async(article)
            .await
            .map_err(|e| format!("Failed to queue article: {e}").into())
    }

    /// Get the receiver for worker tasks
    pub fn receiver(&self) -> Receiver<QueuedArticle> {
        self.receiver.clone()
    }
}

/// Article worker pool configuration
pub struct WorkerPool {
    queue: ArticleQueue,
    storage: DynStorage,
    auth: DynAuth,
    config: Arc<RwLock<Config>>,
    worker_count: usize,
}

impl WorkerPool {
    /// Create a new worker pool
    pub fn new(
        queue: ArticleQueue,
        storage: DynStorage,
        auth: DynAuth,
        config: Arc<RwLock<Config>>,
        worker_count: usize,
    ) -> Self {
        Self {
            queue,
            storage,
            auth,
            config,
            worker_count,
        }
    }

    /// Start all worker tasks
    pub async fn start(&self) -> Vec<tokio::task::JoinHandle<()>> {
        let mut handles = Vec::with_capacity(self.worker_count);

        for worker_id in 0..self.worker_count {
            let receiver = self.queue.receiver();
            let storage = self.storage.clone();
            let auth = self.auth.clone();
            let config = self.config.clone();

            let handle = tokio::spawn(async move {
                worker_task(worker_id, receiver, storage, auth, config).await;
            });

            handles.push(handle);
        }

        info!("Started {} article processing workers", self.worker_count);
        handles
    }
}

/// Worker task that processes articles from the queue
async fn worker_task(
    worker_id: usize,
    receiver: Receiver<QueuedArticle>,
    storage: DynStorage,
    auth: DynAuth,
    config: Arc<RwLock<Config>>,
) {
    info!("Article worker {} started", worker_id);

    while let Ok(queued_article) = receiver.recv_async().await {
        debug!("Worker {} processing article", worker_id);

        if let Err(e) = process_article(&queued_article, &storage, &auth, &config).await {
            error!("Worker {} failed to process article: {}", worker_id, e);
        }
    }

    info!("Article worker {} stopped", worker_id);
}

/// Process a single article: comprehensive validation and storage
async fn process_article(
    queued_article: &QueuedArticle,
    storage: &DynStorage,
    auth: &DynAuth,
    config: &Arc<RwLock<Config>>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let article = &queued_article.message;

    // Handle control messages first
    if queued_article.is_control && crate::control::handle_control(article, storage, auth).await? {
        debug!("Processed control message");
        return Ok(());
    }

    // Perform comprehensive validation only if not already done
    if !queued_article.already_validated {
        let cfg_guard = config.read().await;
        crate::handlers::utils::comprehensive_validate_article(
            storage,
            auth,
            &cfg_guard,
            article,
            queued_article.size,
        )
        .await?;
        drop(cfg_guard);
    }

    // Store the article (check if it already exists to avoid duplicates)
    let message_id = article
        .headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("Message-ID"))
        .map(|(_, v)| v.as_str())
        .unwrap_or("");

    if !message_id.is_empty() && storage.get_article_by_id(message_id).await?.is_some() {
        debug!("Article already exists, skipping storage");
        return Ok(());
    }

    storage.store_article(article).await?;
    debug!("Article stored successfully");

    Ok(())
}
