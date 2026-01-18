import { useState } from "react";
import { useTranslation } from "react-i18next";
import { ChevronDown, ChevronRight } from "lucide-react";

import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { cn } from "@/lib/utils";
import { getDriverMetadata } from "@/lib/drivers";

import type { ConnectionFormData } from "./types";
import { SshTunnelSection } from "./SshTunnelSection";

export function AdvancedSection(props: {
	formData: ConnectionFormData;
	onChange: (field: keyof ConnectionFormData, value: string | number | boolean) => void;
}) {
	const { formData, onChange } = props;
	const { t } = useTranslation();
	const [open, setOpen] = useState(false);

	const driverMeta = getDriverMetadata(formData.driver);

	return (
		<div className="rounded-md border border-border bg-background">
			<button
				type="button"
				className={cn(
					"flex w-full items-center justify-between px-4 py-3 text-sm font-medium",
					"hover:bg-muted/30 transition-colors",
				)}
				onClick={() => setOpen((v) => !v)}
			>
				<span className="flex items-center gap-2">
					{open ? <ChevronDown size={16} /> : <ChevronRight size={16} />}
					{t("connection.advanced")}
				</span>
				<span className="text-xs text-muted-foreground">{t("connection.advancedHint")}</span>
			</button>

			{open && (
				<div className="border-t border-border px-4 py-4 space-y-4">
					<div className="space-y-2">
						<Label>{t(driverMeta.databaseFieldLabel)}</Label>
						<Input
							placeholder={formData.driver === "postgres" ? "postgres" : ""}
							value={formData.database}
							onChange={(e) => onChange("database", e.target.value)}
						/>
					</div>

					<div className="flex items-center justify-between rounded-md border border-border bg-background px-3 py-2">
						<Label className="text-sm">{t("connection.useSSL")}</Label>
						<Switch
							checked={formData.ssl}
							onCheckedChange={(checked) => onChange("ssl", checked)}
						/>
					</div>

					<SshTunnelSection formData={formData} onChange={onChange} />
				</div>
			)}
		</div>
	);
}
