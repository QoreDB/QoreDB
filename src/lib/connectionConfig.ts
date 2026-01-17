import type { ConnectionConfig, SavedConnection, SshTunnelConfig } from './tauri';

type HostKeyPolicy = SshTunnelConfig['host_key_policy'];

const HOST_KEY_POLICIES: ReadonlySet<HostKeyPolicy> = new Set([
  'accept_new',
  'strict',
  'insecure_no_check',
]);

function parseHostKeyPolicy(value: string): HostKeyPolicy {
  if (HOST_KEY_POLICIES.has(value as HostKeyPolicy)) {
    return value as HostKeyPolicy;
  }
  throw new Error(
    `SSH tunnel: host_key_policy invalide: "${value}" (attendu: accept_new | strict | insecure_no_check)`
  );
}

function mapSavedSshTunnel(saved: NonNullable<SavedConnection['ssh_tunnel']>): SshTunnelConfig {
  if (saved.auth_type !== 'key') {
    throw new Error('SSH tunnel: seul auth_type="key" est supporté pour le moment.');
  }

  if (!saved.key_path) {
    throw new Error('SSH tunnel: key_path manquant dans la connexion sauvegardée.');
  }

  return {
    host: saved.host,
    port: saved.port,
    username: saved.username,
    auth: {
      Key: {
        private_key_path: saved.key_path,
      },
    },
    host_key_policy: parseHostKeyPolicy(saved.host_key_policy),
    proxy_jump: saved.proxy_jump,
    connect_timeout_secs: saved.connect_timeout_secs,
    keepalive_interval_secs: saved.keepalive_interval_secs,
    keepalive_count_max: saved.keepalive_count_max,
  };
}

export function buildConnectionConfigFromSavedConnection(
  connection: SavedConnection,
  password: string
): ConnectionConfig {
  return {
    driver: connection.driver,
    host: connection.host,
    port: connection.port,
    username: connection.username,
    password,
    database: connection.database,
    ssl: connection.ssl,
    environment: connection.environment,
    read_only: connection.read_only,
    ssh_tunnel: connection.ssh_tunnel ? mapSavedSshTunnel(connection.ssh_tunnel) : undefined,
  };
}
