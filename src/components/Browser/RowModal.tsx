import { useState, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { Loader2 } from "lucide-react";
import { toast } from "sonner";

import { 
  TableSchema,
  Value,
  insertRow,
  updateRow,
  Namespace,
  RowData as TauriRowData
} from '../../lib/tauri';
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
} from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";



import { Label } from '../ui/label'
import { Checkbox } from '../ui/checkbox'

interface RowModalProps {
  isOpen: boolean;
  onClose: () => void;
  mode: 'insert' | 'update';
  sessionId: string;
  namespace: Namespace;
  tableName: string;
  schema: TableSchema;
  readOnly?: boolean;
  initialData?: Record<string, Value>;
  onSuccess: () => void;
}

export function RowModal({
  isOpen,
  onClose,
  mode,
  sessionId,
  namespace,
  tableName,
  schema,
  readOnly = false,
  initialData,
  onSuccess
}: RowModalProps) {
	const { t } = useTranslation();
	const [loading, setLoading] = useState(false);
	const [formData, setFormData] = useState<Record<string, string>>({});
	const [nulls, setNulls] = useState<Record<string, boolean>>({});
	const [previewError, setPreviewError] = useState<string | null>(null);

	// Initialize form data
	useEffect(() => {
		if (isOpen) {
			const initialForm: Record<string, string> = {};
			const initialNulls: Record<string, boolean> = {};

			schema.columns.forEach((col) => {
				const val = initialData?.[col.name];

				if (mode === "update" && val !== undefined) {
					if (val === null) {
						initialNulls[col.name] = true;
						initialForm[col.name] = "";
					} else {
						initialNulls[col.name] = false;
						initialForm[col.name] = String(val);
					}
				} else {
					initialForm[col.name] = "";
					if (col.nullable && !col.default_value) {
						initialNulls[col.name] = true;
					} else {
						initialNulls[col.name] = false;
					}
				}
			});

			setFormData(initialForm);
			setNulls(initialNulls);
			setPreviewError(null);
		}
	}, [isOpen, schema, initialData, mode]);

	const handleInputChange = (col: string, value: string) => {
		setFormData((prev) => ({ ...prev, [col]: value }));
		if (nulls[col]) {
			setNulls((prev) => ({ ...prev, [col]: false }));
		}
	};

	const handleNullToggle = (col: string, isNull: boolean) => {
		setNulls((prev) => ({ ...prev, [col]: isNull }));
	};

	const parseValue = (value: string, dataType: string): Value => {
		// Basic type inference/conversion
		const type = dataType.toLowerCase();
		if (
			type.includes("int") ||
			type.includes("serial") ||
			type.includes("float") ||
			type.includes("double") ||
			type.includes("numeric")
		) {
			if (value === "" || value === undefined) return null;
			return Number(value);
		}
		if (type.includes("bool")) {
			return value === "true" || value === "1" || value === "yes";
		}
		// JSON
		if (type.includes("json")) {
			try {
				return JSON.parse(value);
			} catch {
				return value; // specific error handling?
			}
		}
		return value;
	};

	const formatPreviewValue = (value: Value): string => {
		if (value === null) return "NULL";
		if (typeof value === "boolean") return value ? "true" : "false";
		if (typeof value === "number") return String(value);
		if (typeof value === "string") return value;
		return JSON.stringify(value);
	};

	const computePreview = () => {
		const data: Record<string, Value> = {};
		schema.columns.forEach((col) => {
			if (nulls[col.name]) {
				data[col.name] = null;
				return;
			}
			const rawVal = formData[col.name];
			if (rawVal === "" && col.default_value) {
				return;
			}
			data[col.name] = parseValue(rawVal, col.data_type);
		});

		if (mode === "insert") {
			return {
				type: "insert" as const,
				values: Object.entries(data).map(([key, value]) => ({
					key,
					value,
				})),
			};
		}

		const changes = schema.columns.flatMap((col) => {
			if (!(col.name in data)) return [];
			const nextValue = data[col.name];
			const prevValue = initialData?.[col.name];
			const prevSerialized = JSON.stringify(prevValue ?? null);
			const nextSerialized = JSON.stringify(nextValue ?? null);
			if (prevSerialized === nextSerialized) return [];

			return [
				{
					key: col.name,
					previous: prevValue ?? null,
					next: nextValue ?? null,
				},
			];
		});

		return { type: "update" as const, changes };
	};

	const handleSubmit = async (e: React.FormEvent) => {
		e.preventDefault();
		if (readOnly) {
			toast.error(t("environment.blocked"));
			return;
		}
		setPreviewError(null);
		setLoading(true);

		try {
			const data: TauriRowData = { columns: {} };

			schema.columns.forEach((col) => {
				if (nulls[col.name]) {
					data.columns[col.name] = null;
				} else {
					const rawVal = formData[col.name];
					if (rawVal === "" && col.default_value) {
						return;
					}
					data.columns[col.name] = parseValue(rawVal, col.data_type);
				}
			});

			if (mode === "insert") {
				const res = await insertRow(
					sessionId,
					namespace.database,
					namespace.schema,
					tableName,
					data
				);
				if (res.success) {
					const timeMsg = res.result?.execution_time_ms
						? ` (${res.result.execution_time_ms.toFixed(2)}ms)`
						: "";
					toast.success(t("rowModal.insertSuccess") + timeMsg);
					onSuccess();
					onClose();
				} else {
					toast.error(res.error || t("rowModal.insertError"));
				}
			} else {
				// Update
				// Construct Primary Key
				const pkData: TauriRowData = { columns: {} };
				if (!schema.primary_key || schema.primary_key.length === 0) {
					throw new Error("No primary key found for update");
				}

				schema.primary_key.forEach((pk) => {
					// Use initial data for PK components to identify the row
					const val = initialData?.[pk];
					pkData.columns[pk] = val ?? null;
				});

				const res = await updateRow(
					sessionId,
					namespace.database,
					namespace.schema,
					tableName,
					pkData,
					data
				);
				if (res.success) {
					const timeMsg = res.result?.execution_time_ms
						? ` (${res.result.execution_time_ms.toFixed(2)}ms)`
						: "";
					toast.success(t("rowModal.updateSuccess") + timeMsg);
					onSuccess();
					onClose();
				} else {
					toast.error(res.error || t("rowModal.updateError"));
				}
			}
		} catch (err) {
			console.error(err);
			const message = err instanceof Error ? err.message : "Operation failed";
			setPreviewError(message);
			toast.error(message);
		} finally {
			setLoading(false);
		}
	};

	const preview = computePreview();
	const updatePreview = preview.type === "update" ? preview : null;
	const hasPreviewChanges =
		preview.type === "insert" ? true : preview.changes.length > 0;
	const previewIsEmpty =
		preview.type === "insert"
			? preview.values.length === 0
			: preview.changes.length === 0;

	return (
		<Dialog open={isOpen} onOpenChange={onClose}>
			<DialogContent className="max-w-xl max-h-[90vh] overflow-y-auto">
				<DialogHeader>
					<DialogTitle>
						{mode === "insert"
							? t("rowModal.insertTitle")
							: t("rowModal.updateTitle", { table: tableName })}
					</DialogTitle>
				</DialogHeader>

				<form onSubmit={handleSubmit}>
					<div className="grid gap-4 py-4">
						{schema.columns.map((col) => (
							<div key={col.name} className="grid gap-2">
								<div className="flex items-center justify-between">
									<Label htmlFor={col.name} className="flex items-center gap-2">
										{col.name}
										<span className="text-xs text-muted-foreground font-mono font-normal">
											({col.data_type})
										</span>
										{col.is_primary_key && (
											<span className="text-xs bg-yellow-100 text-yellow-800 px-1 rounded dark:bg-yellow-900 dark:text-yellow-100">
												PK
											</span>
										)}
									</Label>

									{col.nullable && (
										<div className="flex items-center space-x-2">
											<Checkbox
												id={`${col.name}-null`}
												checked={nulls[col.name] || false}
												onCheckedChange={(checked) =>
													handleNullToggle(col.name, checked as boolean)
												}
												disabled={readOnly}
											/>
											<label
												htmlFor={`${col.name}-null`}
												className="text-xs font-medium leading-none peer-disabled:cursor-not-allowed peer-disabled:opacity-70 text-muted-foreground"
											>
												NULL
											</label>
										</div>
									)}
								</div>

								<Input
									id={col.name}
									value={formData[col.name] || ""}
									onChange={(e) => handleInputChange(col.name, e.target.value)}
									disabled={nulls[col.name] || readOnly}
									placeholder={col.default_value ? `Default: ${col.default_value}` : ""}
									className="font-mono text-sm"
								/>
							</div>
						))}
					</div>

					{mode === "update" && (
						<div
							className="border rounded-md p-3 mb-4 bg-(--q-accent-soft)"
							style={{ borderColor: "var(--q-accent)" }}
						>
							<div className="text-xs font-semibold uppercase tracking-wide text-(--q-accent)">
								{t("rowModal.previewTitle")}
							</div>
							{previewIsEmpty ? (
								<div className="text-xs text-muted-foreground mt-2">
									{t("rowModal.previewEmpty")}
								</div>
							) : (
								<div className="mt-2 space-y-1">
									{(updatePreview?.changes ?? []).map((item) => (
										<div
											key={item.key}
											className="flex items-center justify-between text-xs gap-3"
										>
											<span className="font-mono text-muted-foreground min-w-0">
												{item.key}
											</span>
											<span className="font-mono text-muted-foreground line-through truncate">
												{formatPreviewValue(item.previous)}
											</span>
											<span className="font-mono font-semibold truncate text-(--q-accent-strong)">
												{formatPreviewValue(item.next)}
											</span>
										</div>
									))}
								</div>
							)}
							{previewError && (
								<div className="text-xs text-error mt-2">{previewError}</div>
							)}
						</div>
					)}

					<DialogFooter>
						<Button type="button" variant="outline" onClick={onClose}>
							{t("common.cancel")}
						</Button>
						<Button
							type="submit"
							disabled={loading || readOnly || !hasPreviewChanges}
							title={readOnly ? t("environment.blocked") : undefined}
						>
							{loading && <Loader2 className="mr-2 h-4 w-4 animate-spin" />}
							{mode === "insert" ? t("common.insert") : t("common.save")}
						</Button>
					</DialogFooter>
				</form>
			</DialogContent>
		</Dialog>
	);
}
