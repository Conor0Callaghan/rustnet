use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::{Arc, Mutex};
use std::thread;

use crate::config::Config;
use crate::i18n::I18n;
use crate::network::{Connection, NetworkMonitor, Process};

/// Application actions
pub enum Action {
    Quit,
    Refresh,
    // Add more actions as needed
}

/// Application view modes
pub enum ViewMode {
    Overview,
    ConnectionDetails,
    ProcessDetails,
    Help,
}

/// Application state
pub struct App {
    /// Application configuration
    pub config: Config,
    /// Internationalization
    pub i18n: I18n,
    /// Current view mode
    pub mode: ViewMode,
    // Whether the application should quit - field removed as it was unused, Action::Quit handles this
    /// Network monitor instance
    network_monitor: Option<Arc<Mutex<NetworkMonitor>>>,
    /// Active connections
    pub connections: Vec<Connection>,
    /// Process map (pid to process)
    pub processes: HashMap<u32, Process>,
    /// Currently selected connection
    pub selected_connection: Option<Connection>,
    /// Currently selected connection index
    pub selected_connection_idx: usize,
    // Currently selected process index - field removed as it was unused
    /// Show IP locations (requires MaxMind DB)
    pub show_locations: bool,
    /// Show DNS hostnames instead of IP addresses
    pub show_hostnames: bool,
    // Last connection sort time - field removed as it was unused
    /// Connection order map (for stable ordering)
    connection_order: HashMap<String, usize>,
    /// Next order index for new connections
    next_order_index: usize,
    /// DNS cache to avoid repeated lookups
    dns_cache: HashMap<IpAddr, String>,
}

impl App {
    /// Create a new application instance
    pub fn new(config: Config, i18n: I18n) -> Result<Self> {
        Ok(Self {
            config,
            i18n,
            mode: ViewMode::Overview,
            // should_quit: false, // Field removed
            network_monitor: None,
            connections: Vec::new(),
            processes: HashMap::new(),
            selected_connection: None,
            selected_connection_idx: 0,
            // selected_process_idx: 0, // Field removed
            show_locations: true,
            show_hostnames: false,
            // last_sort_time: std::time::Instant::now(), // Field removed
            connection_order: HashMap::new(),
            next_order_index: 0,
            dns_cache: HashMap::new(),
        })
    }

    /// Start network capture
    pub fn start_capture(&mut self) -> Result<()> {
        // Create network monitor
        let interface = self.config.interface.clone();
        let mut monitor = NetworkMonitor::new(interface)?;

        // Disable process information collection by default for better performance
        monitor.set_collect_process_info(false);

        // Get initial connections without process info
        self.connections = monitor.get_connections()?;

        // Start monitoring in background thread
        let monitor = Arc::new(Mutex::new(monitor));
        let monitor_clone = Arc::clone(&monitor);
        let connections_update = Arc::new(Mutex::new(Vec::new()));
        let connections_update_clone = Arc::clone(&connections_update);

        thread::spawn(move || -> Result<()> {
            loop {
                let mut monitor = monitor_clone.lock().unwrap();
                let new_connections = monitor.get_connections()?;

                // Update shared connections
                let mut connections = connections_update_clone.lock().unwrap();
                *connections = new_connections;

                // Sleep to avoid high CPU usage
                drop(connections);
                drop(monitor);
                thread::sleep(std::time::Duration::from_millis(1000));
            }
        });

        self.network_monitor = Some(monitor);

        Ok(())
    }

    /// Handle key event
    pub fn handle_key(&mut self, key: KeyEvent) -> Option<Action> {
        match self.mode {
            ViewMode::Overview => self.handle_overview_keys(key),
            ViewMode::ConnectionDetails => self.handle_details_keys(key),
            ViewMode::ProcessDetails => self.handle_process_keys(key),
            ViewMode::Help => self.handle_help_keys(key),
        }
    }

    /// Handle keys in overview mode
    fn handle_overview_keys(&mut self, key: KeyEvent) -> Option<Action> {
        match key.code {
            KeyCode::Char('q') => Some(Action::Quit),
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                Some(Action::Quit)
            }
            KeyCode::Char('r') => Some(Action::Refresh),
            KeyCode::Down => {
                if !self.connections.is_empty() {
                    self.selected_connection = Some(
                        self.connections
                            [(self.selected_connection_idx + 1) % self.connections.len()]
                        .clone(),
                    );
                    self.selected_connection_idx =
                        (self.selected_connection_idx + 1) % self.connections.len();
                }
                None
            }
            KeyCode::Up => {
                if !self.connections.is_empty() {
                    self.selected_connection = Some(
                        self.connections[self
                            .selected_connection_idx
                            .checked_sub(1)
                            .unwrap_or(self.connections.len() - 1)]
                        .clone(),
                    );
                    self.selected_connection_idx = self
                        .selected_connection_idx
                        .checked_sub(1)
                        .unwrap_or(self.connections.len() - 1);
                }
                None
            }
            KeyCode::Enter => {
                if !self.connections.is_empty() {
                    self.mode = ViewMode::ConnectionDetails;
                }
                None
            }
            KeyCode::Char('h') => {
                self.mode = ViewMode::Help;
                None
            }
            KeyCode::Char('l') => {
                self.show_locations = !self.show_locations;
                None
            }
            KeyCode::Char('d') => {
                self.show_hostnames = !self.show_hostnames;
                // Clear DNS cache when toggling off to ensure fresh lookups when toggled on again
                if !self.show_hostnames {
                    self.dns_cache.clear();
                }
                None
            }
            _ => None,
        }
    }

    /// Handle keys in connection details mode
    fn handle_details_keys(&mut self, key: KeyEvent) -> Option<Action> {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                self.mode = ViewMode::Overview;
                None
            }
            KeyCode::Char('p') => {
                self.mode = ViewMode::ProcessDetails;
                None
            }
            _ => None,
        }
    }

    /// Handle keys in process details mode
    fn handle_process_keys(&mut self, key: KeyEvent) -> Option<Action> {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                self.mode = ViewMode::ConnectionDetails;
                None
            }
            _ => None,
        }
    }

    /// Handle keys in help mode
    fn handle_help_keys(&mut self, key: KeyEvent) -> Option<Action> {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('h') => {
                self.mode = ViewMode::Overview;
                None
            }
            _ => None,
        }
    }

    /// Update application state on tick
    pub fn on_tick(&mut self) -> Result<()> {
        // Store currently selected connection (if any)
        let selected = self.selected_connection.clone();

        // Update connections from network monitor if available
        if let Some(monitor_arc) = &self.network_monitor {
            let mut monitor = monitor_arc.lock().unwrap(); // Lock the mutex
            let mut new_connections = monitor.get_connections()?;
            drop(monitor); // Release the mutex lock before self-mutation

            // Extract keys for sorting
            let mut keys_to_process = Vec::new();
            for conn in &new_connections {
                let key = self.get_connection_key(conn);
                keys_to_process.push(key);
            }

            // Update connection order
            for key in keys_to_process {
                if !self.connection_order.contains_key(&key) {
                    self.connection_order.insert(key, self.next_order_index);
                    self.next_order_index += 1;
                }
            }

            // Sort connections by their assigned order
            new_connections.sort_by(|a, b| {
                let key_a = self.get_connection_key(a);
                let key_b = self.get_connection_key(b);

                let order_a = self.connection_order.get(&key_a).unwrap_or(&usize::MAX);
                let order_b = self.connection_order.get(&key_b).unwrap_or(&usize::MAX);

                order_a.cmp(order_b)
            });

            // Update connections with the sorted list
            self.connections = new_connections;

            // Restore selected connection position if possible
            if let Some(ref conn) = selected {
                if let Some(idx) = self.find_connection_index(conn) {
                    self.selected_connection_idx = idx;
                    self.selected_connection = Some(self.connections[idx].clone());
                } else if !self.connections.is_empty() {
                    // If previously selected connection is gone, select first one
                    self.selected_connection_idx = 0;
                    self.selected_connection = Some(self.connections[0].clone());
                } else {
                    // If no connections left, clear selection
                    self.selected_connection_idx = 0;
                    self.selected_connection = None;
                }
            } else if !self.connections.is_empty() && self.selected_connection.is_none() {
                // If no previous selection but we have connections, select the first one
                self.selected_connection_idx = 0;
                self.selected_connection = Some(self.connections[0].clone());
            }
        }

        Ok(())
    }

    /// Refresh application data
    pub fn refresh(&mut self) -> Result<()> {
        // Store currently selected connection (if any)
        let selected = self.selected_connection.clone();

        if let Some(monitor_arc) = &self.network_monitor {
            let mut monitor = monitor_arc.lock().unwrap(); // Lock the mutex
            let mut new_connections = monitor.get_connections()?;
            drop(monitor); // Release the mutex lock before self-mutation

            // Extract keys for sorting
            let mut keys_to_process = Vec::new();
            for conn in &new_connections {
                let key = self.get_connection_key(conn);
                keys_to_process.push(key);
            }

            // Update connection order
            for key in keys_to_process {
                if !self.connection_order.contains_key(&key) {
                    self.connection_order.insert(key, self.next_order_index);
                    self.next_order_index += 1;
                }
            }

            // Sort connections by their assigned order
            new_connections.sort_by(|a, b| {
                let key_a = self.get_connection_key(a);
                let key_b = self.get_connection_key(b);

                let order_a = self.connection_order.get(&key_a).unwrap_or(&usize::MAX);
                let order_b = self.connection_order.get(&key_b).unwrap_or(&usize::MAX);

                order_a.cmp(order_b)
            });

            // Update connections with the sorted list
            self.connections = new_connections;

            // Restore selected connection position if possible
            if let Some(ref conn) = selected {
                if let Some(idx) = self.find_connection_index(conn) {
                    self.selected_connection_idx = idx;
                    self.selected_connection = Some(self.connections[idx].clone());
                } else if !self.connections.is_empty() {
                    // If previously selected connection is gone, select first one
                    self.selected_connection_idx = 0;
                    self.selected_connection = Some(self.connections[0].clone());
                } else {
                    // If no connections left, clear selection
                    self.selected_connection_idx = 0;
                    self.selected_connection = None;
                }
            }
        }

        Ok(())
    }

    /// Get process info for selected connection
    pub fn get_process_for_selected_connection(&mut self) -> Option<Process> {
        if self.connections.is_empty() || self.selected_connection_idx >= self.connections.len() {
            return None;
        }

        // Get the selected connection
        let connection = &mut self.connections[self.selected_connection_idx].clone();

        // Check if we already have process info in our local cache
        if let Some(pid) = connection.pid {
            if let Some(process) = self.processes.get(&pid) {
                return Some(process.clone());
            }
        }

        // Otherwise, look it up on demand
        if let Some(monitor_arc) = &self.network_monitor {
            let monitor = monitor_arc.lock().unwrap();

            // Look up the process info for this specific connection
            if let Some(process) = monitor.get_platform_process_for_connection(connection) {
                // Update our local cache
                let pid = process.pid;
                self.processes.insert(pid, process.clone());

                // Update the connection in our list
                if self.selected_connection_idx < self.connections.len() {
                    self.connections[self.selected_connection_idx].pid = Some(pid);
                    self.connections[self.selected_connection_idx].process_name =
                        Some(self.processes[&pid].name.clone());
                }

                return Some(process);
            }
        }

        None
    }

    /// Generate a unique key for a connection
    fn get_connection_key(&self, conn: &Connection) -> String {
        format!(
            "{:?}-{}-{}-{:?}",
            conn.protocol, conn.local_addr, conn.remote_addr, conn.state
        )
    }

    /// Find the index of a connection that matches the selected connection
    fn find_connection_index(&self, selected: &Connection) -> Option<usize> {
        let selected_key = self.get_connection_key(selected);

        for (i, conn) in self.connections.iter().enumerate() {
            let key = self.get_connection_key(conn);
            if key == selected_key {
                return Some(i);
            }
        }

        None
    }

    /// Format a socket address with hostname if enabled (without mutating self)
    pub fn format_socket_addr(&self, addr: std::net::SocketAddr) -> String {
        if self.show_hostnames {
            let ip = addr.ip();
            // Check if it's in the cache
            if let Some(hostname) = self.dns_cache.get(&ip) {
                return format!("{}:{}", hostname, addr.port());
            }

            // Special handling without cache insertion
            if ip.is_loopback() {
                return format!("localhost:{}", addr.port());
            }

            if ip.is_unspecified() {
                return format!("*:{}", addr.port());
            }

            // Just return the address as string if not in cache
            addr.to_string()
        } else {
            addr.to_string()
        }
    }
}
