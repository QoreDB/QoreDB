import type { Environment } from "@/lib/tauri";
import type { Driver } from "@/lib/drivers";

export interface ConnectionFormData {
	name: string;
	driver: Driver;
	environment: Environment;
	readOnly: boolean;
	host: string;
	port: number;
	username: string;
	password: string;
	database: string;
	ssl: boolean;
	useSshTunnel: boolean;
	sshHost: string;
	sshPort: number;
	sshUsername: string;
	sshKeyPath: string;
	sshPassphrase: string;
	sshHostKeyPolicy: "accept_new" | "strict" | "insecure_no_check";
	sshProxyJump: string;
	sshConnectTimeoutSecs: number;
	sshKeepaliveIntervalSecs: number;
	sshKeepaliveCountMax: number;
}

export const initialConnectionFormData: ConnectionFormData = {
	name: "",
	driver: "postgres",
	environment: "development",
	readOnly: false,
	host: "localhost",
	port: 5432,
	username: "",
	password: "",
	database: "",
	ssl: false,
	useSshTunnel: false,
	sshHost: "",
	sshPort: 22,
	sshUsername: "",
	sshKeyPath: "",
	sshPassphrase: "",
	sshHostKeyPolicy: "accept_new",
	sshProxyJump: "",
	sshConnectTimeoutSecs: 10,
	sshKeepaliveIntervalSecs: 30,
	sshKeepaliveCountMax: 3,
};
