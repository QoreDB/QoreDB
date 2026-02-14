import type { Environment } from '@/lib/tauri';
import { Driver } from '@/lib/drivers';

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
  poolMaxConnections: number;
  poolMinConnections: number;
  poolAcquireTimeoutSecs: number;
  useSshTunnel: boolean;
  sshHost: string;
  sshPort: number;
  sshUsername: string;
  sshKeyPath: string;
  sshPassphrase: string;
  sshHostKeyPolicy: 'accept_new' | 'strict' | 'insecure_no_check';
  sshProxyJump: string;
  sshConnectTimeoutSecs: number;
  sshKeepaliveIntervalSecs: number;
  sshKeepaliveCountMax: number;
  // URL mode fields
  useUrl: boolean;
  connectionUrl: string;
}

export const initialConnectionFormData: ConnectionFormData = {
  name: '',
  driver: Driver.Postgres,
  environment: 'development',
  readOnly: false,
  host: 'localhost',
  port: 5432,
  username: '',
  password: '',
  database: '',
  ssl: false,
  poolMaxConnections: 5,
  poolMinConnections: 0,
  poolAcquireTimeoutSecs: 30,
  useSshTunnel: false,
  sshHost: '',
  sshPort: 22,
  sshUsername: '',
  sshKeyPath: '',
  sshPassphrase: '',
  sshHostKeyPolicy: 'accept_new',
  sshProxyJump: '',
  sshConnectTimeoutSecs: 10,
  sshKeepaliveIntervalSecs: 30,
  sshKeepaliveCountMax: 3,
  // URL mode defaults
  useUrl: false,
  connectionUrl: '',
};
