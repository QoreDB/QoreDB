export type Driver = 'postgres' | 'mysql' | 'mongodb';

export const DRIVER_LABELS: Record<Driver, string> = {
  postgres: 'PostgreSQL',
  mysql: 'MySQL / MariaDB',
  mongodb: 'MongoDB',
};

export const DRIVER_ICONS: Record<Driver, string> = {
  postgres: 'postgresql.png',
  mysql: 'mysql.png',
  mongodb: 'mongodb.png',
};

export const DEFAULT_PORTS: Record<Driver, number> = {
  postgres: 5432,
  mysql: 3306,
  mongodb: 27017,
};