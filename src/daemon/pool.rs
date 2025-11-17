//! LSP client pool management.
//!
//! This module manages a pool of TyLspClient instances, one per workspace.
//! Each client maintains a persistent connection to a ty LSP server process,
//! allowing for fast response times on subsequent requests.

#![allow(dead_code)]

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use anyhow::{Context, Result};

use crate::lsp::client::TyLspClient;

/// Entry in the LSP client pool, tracking the client and its last access time.
struct PoolEntry {
    /// The LSP client instance
    client: Arc<TyLspClient>,
    /// Last time this client was accessed
    last_access: Instant,
}

/// Manages a pool of LSP clients, one per workspace.
///
/// The pool maintains persistent connections to ty LSP servers for different
/// workspaces, enabling fast response times by reusing connections across
/// multiple CLI invocations.
///
/// # Thread Safety
///
/// This struct uses internal mutability with Arc<Mutex<...>> to allow safe
/// concurrent access from multiple threads or async tasks.
///
/// # Example
///
/// ```no_run
/// use std::path::PathBuf;
/// use ty_find::daemon::pool::LspClientPool;
///
/// # async fn example() -> anyhow::Result<()> {
/// let pool = LspClientPool::new();
/// let workspace = PathBuf::from("/path/to/workspace");
///
/// // Get or create a client for the workspace
/// let client = pool.get_or_create(workspace).await?;
///
/// // Use the client for LSP operations
/// let locations = client.goto_definition("file.py", 10, 5).await?;
/// # Ok(())
/// # }
/// ```
pub struct LspClientPool {
    /// Map of workspace paths to LSP client entries
    entries: Arc<Mutex<HashMap<PathBuf, PoolEntry>>>,
}

impl LspClientPool {
    /// Creates a new empty LSP client pool.
    ///
    /// # Example
    ///
    /// ```
    /// use ty_find::daemon::pool::LspClientPool;
    ///
    /// let pool = LspClientPool::new();
    /// ```
    pub fn new() -> Self {
        Self {
            entries: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Gets an existing LSP client for the workspace, or creates a new one if it doesn't exist.
    ///
    /// This method updates the last access time for the workspace, which is used
    /// by `cleanup_idle()` to determine which clients to remove.
    ///
    /// # Arguments
    ///
    /// * `workspace` - The workspace root path
    ///
    /// # Returns
    ///
    /// An `Arc<TyLspClient>` that can be shared across threads and used to
    /// communicate with the LSP server for this workspace.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The workspace path is invalid
    /// - The LSP server fails to start
    /// - The LSP initialization fails
    ///
    /// # Example
    ///
    /// ```no_run
    /// use std::path::PathBuf;
    /// use ty_find::daemon::pool::LspClientPool;
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// let pool = LspClientPool::new();
    /// let workspace = PathBuf::from("/path/to/workspace");
    ///
    /// let client = pool.get_or_create(workspace).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn get_or_create(&self, workspace: PathBuf) -> Result<Arc<TyLspClient>> {
        // First, check if we already have a client for this workspace
        {
            let mut entries = self.entries.lock().unwrap();
            if let Some(entry) = entries.get_mut(&workspace) {
                // Update last access time
                entry.last_access = Instant::now();
                return Ok(Arc::clone(&entry.client));
            }
        }

        // No existing client, create a new one
        let workspace_str = workspace
            .to_str()
            .context("Invalid workspace path")?;

        let client = TyLspClient::new(workspace_str)
            .await
            .context("Failed to create LSP client")?;

        // Start the response handler for this client
        client.start_response_handler()
            .await
            .context("Failed to start response handler")?;

        let client_arc = Arc::new(client);

        // Store the client in the pool
        {
            let mut entries = self.entries.lock().unwrap();
            entries.insert(
                workspace.clone(),
                PoolEntry {
                    client: Arc::clone(&client_arc),
                    last_access: Instant::now(),
                },
            );
        }

        Ok(client_arc)
    }

    /// Removes the LSP client for the specified workspace from the pool.
    ///
    /// This will shut down the LSP server connection for that workspace.
    /// If the workspace is not in the pool, this is a no-op.
    ///
    /// # Arguments
    ///
    /// * `workspace` - The workspace root path
    ///
    /// # Example
    ///
    /// ```no_run
    /// use std::path::PathBuf;
    /// use ty_find::daemon::pool::LspClientPool;
    ///
    /// let pool = LspClientPool::new();
    /// let workspace = PathBuf::from("/path/to/workspace");
    ///
    /// pool.remove(&workspace);
    /// ```
    pub fn remove(&self, workspace: &Path) {
        let mut entries = self.entries.lock().unwrap();
        entries.remove(workspace);
    }

    /// Removes all LSP clients that haven't been accessed within the specified timeout.
    ///
    /// This method is useful for cleaning up idle connections to free resources.
    /// It should be called periodically (e.g., every minute) to remove stale clients.
    ///
    /// # Arguments
    ///
    /// * `timeout` - The idle timeout duration. Clients that haven't been accessed
    ///   for longer than this will be removed.
    ///
    /// # Returns
    ///
    /// The number of clients that were removed.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use std::time::Duration;
    /// use ty_find::daemon::pool::LspClientPool;
    ///
    /// let pool = LspClientPool::new();
    ///
    /// // Remove clients idle for more than 5 minutes
    /// let removed = pool.cleanup_idle(Duration::from_secs(300));
    /// println!("Removed {} idle clients", removed);
    /// ```
    pub fn cleanup_idle(&self, timeout: Duration) -> usize {
        let mut entries = self.entries.lock().unwrap();
        let now = Instant::now();

        let to_remove: Vec<PathBuf> = entries
            .iter()
            .filter(|(_, entry)| now.duration_since(entry.last_access) > timeout)
            .map(|(path, _)| path.clone())
            .collect();

        let count = to_remove.len();
        for path in to_remove {
            entries.remove(&path);
        }

        count
    }

    /// Returns a list of all active workspace paths in the pool.
    ///
    /// The workspaces are returned in arbitrary order.
    ///
    /// # Returns
    ///
    /// A vector of workspace paths that have active LSP clients.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use ty_find::daemon::pool::LspClientPool;
    ///
    /// let pool = LspClientPool::new();
    ///
    /// let workspaces = pool.active_workspaces();
    /// for workspace in workspaces {
    ///     println!("Active workspace: {}", workspace.display());
    /// }
    /// ```
    pub fn active_workspaces(&self) -> Vec<PathBuf> {
        let entries = self.entries.lock().unwrap();
        entries.keys().cloned().collect()
    }

    /// Returns the number of active LSP clients in the pool.
    ///
    /// # Example
    ///
    /// ```
    /// use ty_find::daemon::pool::LspClientPool;
    ///
    /// let pool = LspClientPool::new();
    /// assert_eq!(pool.len(), 0);
    /// ```
    pub fn len(&self) -> usize {
        let entries = self.entries.lock().unwrap();
        entries.len()
    }

    /// Returns true if the pool has no active clients.
    ///
    /// # Example
    ///
    /// ```
    /// use ty_find::daemon::pool::LspClientPool;
    ///
    /// let pool = LspClientPool::new();
    /// assert!(pool.is_empty());
    /// ```
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Default for LspClientPool {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_new_pool_is_empty() {
        let pool = LspClientPool::new();
        assert_eq!(pool.len(), 0);
        assert!(pool.is_empty());
    }

    #[test]
    fn test_active_workspaces_empty() {
        let pool = LspClientPool::new();
        let workspaces = pool.active_workspaces();
        assert!(workspaces.is_empty());
    }

    #[test]
    fn test_remove_nonexistent_workspace() {
        let pool = LspClientPool::new();
        let workspace = PathBuf::from("/nonexistent");

        // Should not panic
        pool.remove(&workspace);
        assert_eq!(pool.len(), 0);
    }

    #[test]
    fn test_cleanup_idle_empty_pool() {
        let pool = LspClientPool::new();
        let removed = pool.cleanup_idle(Duration::from_secs(60));
        assert_eq!(removed, 0);
    }
}
