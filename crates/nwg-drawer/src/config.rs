use clap::{Parser, ValueEnum};

/// A macOS-style application drawer/launcher for Hyprland/Sway.
#[derive(Parser, Debug, Clone)]
#[command(name = "nwg-drawer", version, about)]
pub struct DrawerConfig {
    /// CSS file name
    #[arg(short = 's', long, default_value = "drawer.css")]
    pub css_file: String,

    /// Name of output to display on
    #[arg(short = 'o', long, default_value = "")]
    pub output: String,

    /// Use overlay layer (otherwise top)
    #[arg(long, alias = "ovl")]
    pub overlay: bool,

    /// Window background opacity 0-100 (default: 88)
    #[arg(long, default_value_t = 88)]
    pub opacity: u8,

    /// GTK theme name
    #[arg(short = 'g', long, default_value = "")]
    pub gtk_theme: String,

    /// GTK icon theme name
    #[arg(short = 'i', long, default_value = "")]
    pub icon_theme: String,

    /// Icon size in pixels
    #[arg(long, alias = "is", default_value_t = 64)]
    pub icon_size: i32,

    /// Number of columns in the app grid
    #[arg(short = 'c', long, default_value_t = 6)]
    pub columns: u32,

    /// Icon spacing
    #[arg(long, default_value_t = 20)]
    pub spacing: u32,

    /// Force lang (e.g. "en", "pl")
    #[arg(long, default_value = "")]
    pub lang: String,

    /// File manager command
    #[arg(long, alias = "fm", default_value = "thunar")]
    pub file_manager: String,

    /// Terminal emulator
    #[arg(long, default_value = "foot")]
    pub term: String,

    /// File search name length limit
    #[arg(long, alias = "fslen", default_value_t = 80)]
    pub fs_name_limit: usize,

    /// Disable category filtering
    #[arg(long, alias = "nocats")]
    pub no_cats: bool,

    /// Disable file search
    #[arg(long, alias = "nofs")]
    pub no_fs: bool,

    /// Maximum number of file search results
    #[arg(long, default_value_t = 25)]
    pub fs_max_results: usize,

    /// Leave the program resident in memory
    #[arg(short = 'r', long)]
    pub resident: bool,

    /// File search result columns
    #[arg(long, alias = "fscol", default_value_t = 2)]
    pub fs_columns: u32,

    /// Margin top
    #[arg(long, default_value_t = 0)]
    pub mt: i32,

    /// Margin left
    #[arg(long, default_value_t = 0)]
    pub ml: i32,

    /// Margin right
    #[arg(long, default_value_t = 0)]
    pub mr: i32,

    /// Margin bottom
    #[arg(long, default_value_t = 0)]
    pub mb: i32,

    /// Auto-detect power bar buttons from system capabilities
    #[arg(long, alias = "pbauto")]
    pub pb_auto: bool,

    /// Power bar exit command
    #[arg(long, alias = "pbexit", default_value = "")]
    pub pb_exit: String,

    /// Power bar lock command
    #[arg(long, alias = "pblock", default_value = "")]
    pub pb_lock: String,

    /// Power bar poweroff command
    #[arg(long, alias = "pbpoweroff", default_value = "")]
    pub pb_poweroff: String,

    /// Power bar reboot command
    #[arg(long, alias = "pbreboot", default_value = "")]
    pub pb_reboot: String,

    /// Power bar sleep command
    #[arg(long, alias = "pbsleep", default_value = "")]
    pub pb_sleep: String,

    /// Power bar icon size
    #[arg(long, alias = "pbsize", default_value_t = 64)]
    pub pb_size: i32,

    /// Use icon theme for power bar (instead of built-in)
    #[arg(long, alias = "pbuseicontheme")]
    pub pb_use_icon_theme: bool,

    /// Turn on debug messages
    #[arg(short = 'd', long)]
    pub debug: bool,

    /// Set keyboard interactivity to on-demand
    #[arg(short = 'k', long)]
    pub keyboard_on_demand: bool,

    /// Close button position
    #[arg(long, value_enum, default_value_t = CloseButton::None)]
    pub closebtn: CloseButton,

    /// Open a running resident instance
    #[arg(long)]
    pub open: bool,

    /// Close a running resident instance
    #[arg(long)]
    pub close: bool,

    /// Force GTK theme for libadwaita apps (prepends GTK_THEME= to launch commands)
    #[arg(long, alias = "ft")]
    pub force_theme: bool,

    /// Window manager override (auto-detected from environment if not specified)
    #[arg(long, default_value = "")]
    pub wm: String,
}

/// Close button position in the drawer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum CloseButton {
    Left,
    Right,
    None,
}

impl DrawerConfig {
    /// Whether any power bar button is configured.
    pub fn has_power_bar(&self) -> bool {
        !self.pb_exit.is_empty()
            || !self.pb_lock.is_empty()
            || !self.pb_poweroff.is_empty()
            || !self.pb_reboot.is_empty()
            || !self.pb_sleep.is_empty()
    }
}
