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
		<div className="grid grid-cols-2 sm:grid-cols-3 gap-4">
			{(Object.keys(DRIVER_LABELS) as Driver[]).map((d) => (
				<button
					key={d}
					type="button"
					className={cn(
						"flex flex-col items-center gap-3 p-4 rounded-xl border-2 transition-all hover:scale-[1.02] active:scale-[0.98]",
						driver === d
							? "border-accent bg-accent/5"
							: "border-border bg-background hover:border-foreground/20 hover:bg-muted/50",
					)}
					onClick={() => onChange(d)}
					disabled={isEditMode}
				>
					<div
						className={cn(
							"flex items-center justify-center w-16 h-16 rounded-2xl p-3 transition-colors shadow-sm",
							driver === d ? "bg-accent/10" : "bg-muted",
						)}
					>
						<img
							src={`/databases/${DRIVER_ICONS[d]}`}
							alt={DRIVER_LABELS[d]}
							className="w-full h-full object-contain"
						/>
					</div>
					<span className={cn(
            "text-sm font-semibold",
            driver === d ? "text-accent" : "text-foreground"
          )}>{DRIVER_LABELS[d]}</span>
				</button>
			))}
		</div>
	);
}
