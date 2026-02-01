import { useCallback, useEffect, useMemo, useState } from "react";

import type { PartialConnectionConfig, SavedConnection } from "@/lib/tauri";
import { DEFAULT_PORTS, Driver } from "@/lib/drivers";

import { initialConnectionFormData, type ConnectionFormData } from "./types";
import { isConnectionFormValid } from "./mappers";

/**
 * Maps a driver string from URL parsing to the Driver enum
 */
function mapDriverString(driver: string | undefined): Driver | undefined {
	if (!driver) return undefined;
	const normalized = driver.toLowerCase();
	switch (normalized) {
    case 'postgres':
    case 'postgresql':
      return Driver.Postgres;
    case 'mysql':
      return Driver.Mysql;
    case 'mongodb':
      return Driver.Mongodb;
    case 'sqlite':
      return Driver.Sqlite;
    default:
      return undefined;
  }
}

export function useConnectionForm(options: {
	isOpen: boolean;
	editConnection?: SavedConnection;
	editPassword?: string;
}) {
	const { isOpen, editConnection, editPassword } = options;
	const [formData, setFormData] = useState<ConnectionFormData>(
		initialConnectionFormData,
	);

	useEffect(() => {
		if (!isOpen) return;

		if (editConnection && editPassword) {
			const sshTunnel = editConnection.ssh_tunnel;
			setFormData({
				name: editConnection.name,
				driver: editConnection.driver as Driver,
				environment: editConnection.environment || "development",
				readOnly: editConnection.read_only || false,
				host: editConnection.host,
				port: editConnection.port,
				username: editConnection.username,
				password: editPassword,
				database: editConnection.database || "",
				ssl: editConnection.ssl,
				poolMaxConnections: editConnection.pool_max_connections ?? 5,
				poolMinConnections: editConnection.pool_min_connections ?? 0,
				poolAcquireTimeoutSecs: editConnection.pool_acquire_timeout_secs ?? 30,
				useSshTunnel: !!sshTunnel,
				sshHost: sshTunnel ? sshTunnel.host : "",
				sshPort: sshTunnel ? sshTunnel.port : 22,
				sshUsername: sshTunnel ? sshTunnel.username : "",
				sshKeyPath: sshTunnel ? sshTunnel.key_path || "" : "",
				sshPassphrase: "",
				sshHostKeyPolicy: sshTunnel
					? (sshTunnel.host_key_policy as ConnectionFormData["sshHostKeyPolicy"])
					: "accept_new",
				sshProxyJump: sshTunnel ? sshTunnel.proxy_jump || "" : "",
				sshConnectTimeoutSecs: sshTunnel ? sshTunnel.connect_timeout_secs : 10,
				sshKeepaliveIntervalSecs: sshTunnel
					? sshTunnel.keepalive_interval_secs
					: 30,
				sshKeepaliveCountMax: sshTunnel ? sshTunnel.keepalive_count_max : 3,
				useUrl: false,
				connectionUrl: "",
			});
		} else {
			setFormData(initialConnectionFormData);
		}
	}, [isOpen, editConnection, editPassword]);

	function handleDriverChange(driver: Driver) {
		setFormData((prev) => ({
			...prev,
			driver,
			port: DEFAULT_PORTS[driver],
		}));
	}

	function handleChange(field: keyof ConnectionFormData, value: string | number | boolean) {
		setFormData((prev) => ({ ...prev, [field]: value }));
	}

	/**
	 * Apply parsed URL configuration to form fields.
	 * URL-derived values are applied, but existing non-empty values for name,
	 * environment, readOnly, and pool settings are preserved (user overrides).
	 */
	const applyParsedConfig = useCallback((config: PartialConnectionConfig) => {
		setFormData((prev) => {
			const driver = mapDriverString(config.driver) ?? prev.driver;
			const port = config.port ?? DEFAULT_PORTS[driver];

			return {
				...prev,
				// Apply URL-derived values
				driver,
				host: config.host ?? prev.host,
				port,
				username: config.username ?? prev.username,
				password: config.password ?? prev.password,
				database: config.database ?? prev.database,
				ssl: config.ssl ?? prev.ssl,
			};
		});
	}, []);

	const isValid = useMemo(() => isConnectionFormValid(formData), [formData]);

	return {
		formData,
		setFormData,
		handleDriverChange,
		handleChange,
		applyParsedConfig,
		isValid,
	};
}
