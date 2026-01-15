import { SavedConnection } from '../../lib/tauri';
import { Loader2, ChevronRight, ChevronDown } from 'lucide-react';
import { cn } from '@/lib/utils';
import { Driver, DRIVER_ICONS, DRIVER_LABELS } from '../../lib/drivers';
import { ConnectionMenu } from '../Connection/ConnectionMenu';
import { ConnectionContextMenu } from "../Connection/ConnectionContextMenu";
import { ENVIRONMENT_CONFIG } from "../../lib/environment";

interface ConnectionItemProps {
	connection: SavedConnection;
	isSelected: boolean;
	isExpanded: boolean;
	isConnected?: boolean;
	isConnecting?: boolean;
	onSelect: () => void;
	onEdit: (connection: SavedConnection, password: string) => void;
	onDeleted: () => void;
}

export function ConnectionItem({
	connection,
	isSelected,
	isExpanded,
	isConnected,
	isConnecting,
	onSelect,
	onEdit,
	onDeleted,
}: ConnectionItemProps) {
	const driver = connection.driver as Driver;
	const iconSrc = `/databases/${DRIVER_ICONS[driver]}`;
	const env = connection.environment || "development";
	const envConfig = ENVIRONMENT_CONFIG[env];
	const isProduction = env === "production";

	return (
		<ConnectionContextMenu
			connection={connection}
			onEdit={onEdit}
			onDeleted={onDeleted}
		>
			<div
				className={cn(
					"group flex items-center transition-all",
					isProduction ? "rounded-r-md rounded-l-none" : "rounded-md",
					isSelected && !isConnected && "bg-muted text-foreground",
					isSelected &&
						isConnected &&
						"bg-(--q-accent-soft) text-(--q-accent) font-medium",
					!isSelected &&
						"text-muted-foreground hover:bg-accent/10 hover:text-accent-foreground"
				)}
				style={{
					borderLeft: isProduction ? `3px solid ${envConfig.color}` : undefined,
					paddingLeft: isProduction ? undefined : "3px",
				}}
			>
				<button
					className={cn(
						"flex-1 flex items-center gap-2 px-2 py-1.5 text-sm select-none text-inherit",
						isProduction ? "rounded-l-none" : "rounded-l-md"
					)}
					onClick={onSelect}
					disabled={isConnecting}
				>
					<div className="shrink-0 w-4 h-4 rounded-sm overflow-hidden bg-background/50 p-0.5">
						<img
							src={iconSrc}
							alt={DRIVER_LABELS[driver]}
							className="w-full h-full object-contain"
						/>
					</div>

					<span className="flex-1 truncate text-left">{connection.name}</span>

					{env !== "development" && (
						<span
							className="px-1.5 py-0.5 text-[10px] font-bold rounded"
							style={{
								backgroundColor: envConfig.bgSoft,
								color: envConfig.color,
							}}
						>
							{envConfig.labelShort}
						</span>
					)}

					{isConnecting ? (
						<Loader2 size={14} className="animate-spin text-muted-foreground" />
					) : isConnected && !isConnecting ? (
						<span className="w-2 h-2 rounded-full bg-success shadow-sm shadow-success/50" />
					) : null}

					<div
						className={cn(
							"text-muted-foreground/50",
							isExpanded && "transform rotate-90"
						)}
					>
						{isExpanded ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
					</div>
				</button>

				<ConnectionMenu
					connection={connection}
					onEdit={onEdit}
					onDeleted={onDeleted}
				/>
			</div>
		</ConnectionContextMenu>
	);
}
