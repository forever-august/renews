//! Connection session state management

/// Encapsulated session state for a client connection
pub struct Session {
    current_group: Option<String>,
    current_article: Option<u64>,
    authenticated: bool,
    username: Option<String>,
    is_tls: bool,
    in_stream_mode: bool,
    allow_posting_insecure: bool,
}

impl Session {
    pub fn new(is_tls: bool, allow_posting_insecure: bool) -> Self {
        Self {
            current_group: None,
            current_article: None,
            authenticated: false,
            username: None,
            is_tls,
            in_stream_mode: false,
            allow_posting_insecure,
        }
    }

    // Group management
    pub fn select_group(&mut self, group: String, first_article: Option<u64>) {
        self.current_group = Some(group);
        self.current_article = first_article;
    }

    pub fn current_group(&self) -> Option<&str> {
        self.current_group.as_deref()
    }

    pub fn leave_group(&mut self) {
        self.current_group = None;
        self.current_article = None;
    }

    // Article navigation
    pub fn current_article(&self) -> Option<u64> {
        self.current_article
    }

    pub fn set_current_article(&mut self, num: u64) {
        self.current_article = Some(num);
    }

    // Authentication
    /// Set the pending username for AUTHINFO USER/PASS flow.
    /// Called when USER is received but before PASS is verified.
    pub fn set_pending_username(&mut self, username: String) {
        self.username = Some(username);
    }

    /// Get the pending username set by AUTHINFO USER.
    pub fn pending_username(&self) -> Option<&str> {
        self.username.as_deref()
    }

    pub fn authenticate(&mut self, username: String) {
        self.authenticated = true;
        self.username = Some(username);
    }

    /// Mark the session as authenticated (username should already be set).
    pub fn confirm_authentication(&mut self) {
        self.authenticated = true;
    }

    pub fn is_authenticated(&self) -> bool {
        self.authenticated
    }

    pub fn username(&self) -> Option<&str> {
        self.username.as_deref()
    }

    // Posting permissions
    pub fn can_post(&self) -> bool {
        self.authenticated && (self.is_tls || self.allow_posting_insecure)
    }

    /// Check if posting can be attempted (TLS or insecure posting allowed).
    /// This is the pre-authentication check for MODE READER response.
    pub fn allows_posting_attempt(&self) -> bool {
        self.is_tls || self.allow_posting_insecure
    }

    pub fn is_tls(&self) -> bool {
        self.is_tls
    }

    // Stream mode
    pub fn enter_stream_mode(&mut self) {
        self.in_stream_mode = true;
    }

    pub fn is_stream_mode(&self) -> bool {
        self.in_stream_mode
    }
}
