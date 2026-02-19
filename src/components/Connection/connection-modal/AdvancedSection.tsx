// SPDX-License-Identifier: Apache-2.0

import { ChevronDown, ChevronRight } from "lucide-react";
import { useState } from 'react';
import { useTranslation } from "react-i18next";

import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Switch } from "@/components/ui/switch";
import { Driver, getDriverMetadata } from '@/lib/drivers';
import { cn } from "@/lib/utils";
import { SshTunnelSection } from './SshTunnelSection';
import type { ConnectionFormData } from "./types";

interface AdvancedSectionProps {
	formData: ConnectionFormData;
	onChange: (
		field: keyof ConnectionFormData,
		value: string | number | boolean,
	) => void;
	hideUrlDerivedFields?: boolean;
}

export function AdvancedSection({
  formData,
  onChange,
  hideUrlDerivedFields = false,
}: AdvancedSectionProps) {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);

  const driverMeta = getDriverMetadata(formData.driver);
  const parseNumber = (value: string, fallback: number) => {
    const parsed = Number(value);
    return Number.isFinite(parsed) ? parsed : fallback;
  };

  // Check if there's any content to show in advanced section
  const hasPoolSettings = driverMeta.supportsSQL;
  const hasDatabaseField = !hideUrlDerivedFields;
  const hasSslField = !hideUrlDerivedFields;
  const hasContent = hasDatabaseField || hasSslField || hasPoolSettings || true;

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
					<span className="text-xs text-muted-foreground">
						{hideUrlDerivedFields
							? t("connection.advancedHintUrlMode")
							: t("connection.advancedHint")}
					</span>
				</button>

				{open && hasContent && (
					<div className="border-t border-border px-4 py-4 space-y-4">
						{!hideUrlDerivedFields && (
							<div className="space-y-2">
								<Label>{t(driverMeta.databaseFieldLabel)}</Label>
								<Input
									placeholder={formData.driver === Driver.Postgres ? "postgres" : ""}
									value={formData.database}
									onChange={(e) => onChange("database", e.target.value)}
								/>
							</div>
						)}

						{!hideUrlDerivedFields && (
							<div className="flex items-center justify-between rounded-md border border-border bg-background px-3 py-2">
								<Label className="text-sm">{t("connection.useSSL")}</Label>
								<Switch
									checked={formData.ssl}
									onCheckedChange={(checked) => onChange("ssl", checked)}
								/>
							</div>
						)}

						{driverMeta.supportsSQL && (
							<div className="space-y-2">
								<Label>{t("connection.poolSettings")}</Label>
								<div className="grid grid-cols-3 gap-3">
									<div className="space-y-1">
										<Label className="text-xs text-muted-foreground">
											{t("connection.poolMax")}
										</Label>
										<Input
											type="number"
											min={1}
											max={50}
											value={formData.poolMaxConnections}
											onChange={(e) =>
												onChange(
													"poolMaxConnections",
													parseNumber(e.target.value, formData.poolMaxConnections),
												)
											}
										/>
									</div>
									<div className="space-y-1">
										<Label className="text-xs text-muted-foreground">
											{t("connection.poolMin")}
										</Label>
										<Input
											type="number"
											min={0}
											max={50}
											value={formData.poolMinConnections}
											onChange={(e) =>
												onChange(
													"poolMinConnections",
													parseNumber(e.target.value, formData.poolMinConnections),
												)
											}
										/>
									</div>
									<div className="space-y-1">
										<Label className="text-xs text-muted-foreground">
											{t("connection.poolAcquireTimeout")}
										</Label>
										<Input
											type="number"
											min={5}
											max={120}
											value={formData.poolAcquireTimeoutSecs}
											onChange={(e) =>
												onChange(
													"poolAcquireTimeoutSecs",
													parseNumber(e.target.value, formData.poolAcquireTimeoutSecs),
												)
											}
										/>
									</div>
								</div>
							</div>
						)}

						<SshTunnelSection formData={formData} onChange={onChange} />
					</div>
				)}
			</div>
		);
}
