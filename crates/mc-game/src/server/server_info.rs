//! `net.minecraft.server.ServerInfo` and `ServerInterface` (what the query listener / rcon / management API read).
//! `run_command` of `ServerInterface` is `rcon::RconCommandRunner::run`, implemented for the server in
//! `dedicated::dedicated_server`.

pub trait ServerInfo {
    fn get_motd(&self) -> String;
    fn get_server_version(&self) -> String;
    fn get_player_count(&self) -> i32;
    fn get_max_players(&self) -> i32;
}

/// `ServerInterface` minus `getProperties`/`runCommand` (see module docs).
pub trait ServerInterface: ServerInfo {
    fn get_server_ip(&self) -> String;
    fn get_server_port(&self) -> i32;
    fn get_server_name(&self) -> String;
    fn get_player_names(&self) -> Vec<String>;
    fn get_level_id_name(&self) -> String;
    fn get_plugin_names(&self) -> String;
}
