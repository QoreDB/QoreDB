import { useEffect, useMemo, useState } from "react";

import type { SavedConnection } from "@/lib/tauri";
import { DEFAULT_PORTS, type Driver } from "@/lib/drivers";

import { initialConnectionFormData, type ConnectionFormData } from "./types";
import { isConnectionFormValid } from "./mappers";

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

	const isValid = useMemo(() => isConnectionFormValid(formData), [formData]);

	return {
		formData,
		setFormData,
		handleDriverChange,
		handleChange,
		isValid,
	};
}
