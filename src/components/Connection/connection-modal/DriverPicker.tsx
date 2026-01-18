import type { Driver } from "@/lib/drivers";
import { DRIVER_ICONS, DRIVER_LABELS } from "@/lib/drivers";
import { cn } from "@/lib/utils";

export function DriverPicker(props: {
	driver: Driver;
	isEditMode: boolean;
	onChange: (driver: Driver) => void;
}) {
	const { driver, isEditMode, onChange } = props;

	return (
		<div className="grid grid-cols-3 gap-3">
			{(Object.keys(DRIVER_LABELS) as Driver[]).map((d) => (
				<button
					key={d}
					type="button"
					className={cn(
						"flex flex-col items-center gap-2 p-3 rounded-md border transition-all hover:bg-(--q-accent-soft)",
						driver === d
							? "border-accent bg-(--q-accent-soft) text-(--q-accent)"
							: "border-border bg-background",
					)}
					onClick={() => onChange(d)}
					disabled={isEditMode}
				>
					<div
						className={cn(
							"flex items-center justify-center w-10 h-10 rounded-lg p-1.5 transition-colors",
							driver === d ? "bg-(--q-accent-soft)" : "bg-muted",
						)}
					>
						<img
							src={`/databases/${DRIVER_ICONS[d]}`}
							alt={DRIVER_LABELS[d]}
							className="w-full h-full object-contain"
						/>
					</div>
					<span className="text-xs font-medium">{DRIVER_LABELS[d]}</span>
				</button>
			))}
		</div>
	);
}
