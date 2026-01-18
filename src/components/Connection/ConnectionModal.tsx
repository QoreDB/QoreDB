import { useState } from "react";
import { useTranslation } from "react-i18next";

import {
	connectSavedConnection,
	saveConnection,
	testConnection,
	type SavedConnection,
} from "@/lib/tauri";

import { Button } from "@/components/ui/button";
import {
	Dialog,
	DialogContent,
	DialogFooter,
	DialogHeader,
	DialogTitle,
} from "@/components/ui/dialog";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Check, Loader2, X } from "lucide-react";
import { toast } from "sonner";

import { DriverPicker } from "./connection-modal/DriverPicker";
import { BasicSection } from "./connection-modal/BasicSection";
import { AdvancedSection } from "./connection-modal/AdvancedSection";
import { buildConnectionConfig, buildSaveConnectionInput, buildSavedConnection } from "./connection-modal/mappers";
import { useConnectionForm } from "./connection-modal/useConnectionForm";

interface ConnectionModalProps {
	isOpen: boolean;
	onClose: () => void;
	onConnected: (sessionId: string, connection: SavedConnection) => void;
	editConnection?: SavedConnection;
	editPassword?: string;
	onSaved?: (connection: SavedConnection) => void;
}

export function ConnectionModal({
	isOpen,
	onClose,
	onConnected,
	editConnection,
	editPassword,
	onSaved,
}: ConnectionModalProps) {
	const { t } = useTranslation();
	const { formData, handleChange: setField, handleDriverChange, isValid } =
		useConnectionForm({ isOpen, editConnection, editPassword });
	const [testing, setTesting] = useState(false);
	const [connecting, setConnecting] = useState(false);
	const [testResult, setTestResult] = useState<"success" | "error" | null>(null);
	const [error, setError] = useState<string | null>(null);

	const isEditMode = !!editConnection;

	function handleDriverChangeWithReset(nextDriver: Parameters<typeof handleDriverChange>[0]) {
		handleDriverChange(nextDriver);
		setTestResult(null);
		setError(null);
	}

	function handleChange(field: Parameters<typeof setField>[0], value: Parameters<typeof setField>[1]) {
		setField(field, value);
		setTestResult(null);
		setError(null);
	}

	async function handleTestConnection() {
		setTesting(true);
		setTestResult(null);
		setError(null);

		try {
			const config = buildConnectionConfig(formData);

			const result = await testConnection(config);

			if (result.success) {
				setTestResult("success");
				toast.success(t("connection.testSuccess"));
			} else {
				setTestResult("error");
				setError(result.error || t("connection.testFail"));
				toast.error(t("connection.testFail"), { description: result.error });
			}
		} catch (err) {
			setTestResult("error");
			const errorMsg = err instanceof Error ? err.message : t("common.error");
			setError(errorMsg);
			toast.error(t("connection.testFail"), { description: errorMsg });
		} finally {
			setTesting(false);
		}
	}

	async function handleSaveAndConnect() {
		setConnecting(true);
		setError(null);

		try {
			const connectionId = editConnection?.id || `conn_${Date.now()}`;
			const savedConnection = buildSavedConnection(formData, connectionId);
			await saveConnection(buildSaveConnectionInput(formData, connectionId));

			if (isEditMode) {
				toast.success(t("connection.updateSuccess"));
				onSaved?.(savedConnection);
				onClose();
			} else {
				const connectResult = await connectSavedConnection("default", connectionId);

				if (connectResult.success && connectResult.session_id) {
					toast.success(t("connection.connectedSuccess"));
					onConnected(connectResult.session_id, savedConnection);
					onClose();
				} else {
					setError(connectResult.error || t("connection.connectFail"));
					toast.error(t("connection.connectFail"), {
						description: connectResult.error,
					});
				}
			}
		} catch (err) {
			const errorMsg = err instanceof Error ? err.message : t("common.error");
			setError(errorMsg);
			toast.error(t("common.error"), { description: errorMsg });
		} finally {
			setConnecting(false);
		}
	}

	async function handleSaveOnly() {
		setConnecting(true);
		setError(null);

		try {
			const connectionId = editConnection?.id || `conn_${Date.now()}`;
			const savedConnection = buildSavedConnection(formData, connectionId);
			await saveConnection(buildSaveConnectionInput(formData, connectionId));

			toast.success(
				isEditMode ? t("connection.updateSuccess") : t("connection.saveSuccess"),
			);
			onSaved?.(savedConnection);
			onClose();
		} catch (err) {
			const errorMsg = err instanceof Error ? err.message : t("common.error");
			setError(errorMsg);
			toast.error(t("common.error"), { description: errorMsg });
		} finally {
			setConnecting(false);
		}
	}

	function handleOpenChange(open: boolean) {
		if (!open) onClose();
	}

	return (
		<Dialog open={isOpen} onOpenChange={handleOpenChange}>
			<DialogContent className="max-w-xl">
				<DialogHeader>
					<DialogTitle>
						{isEditMode
							? t("connection.modalTitleEdit")
							: t("connection.modalTitleNew")}
					</DialogTitle>
				</DialogHeader>

				<ScrollArea className="max-h-[75vh]">
					<div className="grid gap-4 py-4">
						<DriverPicker
							driver={formData.driver}
							isEditMode={isEditMode}
							onChange={handleDriverChangeWithReset}
						/>
						<BasicSection formData={formData} onChange={handleChange} />
						<AdvancedSection formData={formData} onChange={handleChange} />

						{error && (
							<div className="p-3 rounded-md bg-error/10 border border-error/20 text-error text-sm flex items-center gap-2">
								<X size={14} />
								{error}
							</div>
						)}
						{testResult === "success" && (
							<div className="p-3 rounded-md bg-success/10 border border-success/20 text-success text-sm flex items-center gap-2">
								<Check size={14} />
								{t("connection.testSuccess")}
							</div>
						)}
					</div>
				</ScrollArea>

				<DialogFooter>
					<Button variant="outline" onClick={onClose}>
						{t("connection.cancel")}
					</Button>
					<Button
						variant="secondary"
						onClick={handleTestConnection}
						disabled={!isValid || testing}
					>
						{testing && <Loader2 className="mr-2 h-4 w-4 animate-spin" />}
						{t("connection.test")}
					</Button>
					{isEditMode ? (
						<Button onClick={handleSaveOnly} disabled={!isValid || connecting}>
							{connecting && <Loader2 className="mr-2 h-4 w-4 animate-spin" />}
							{t("connection.saveChanges")}
						</Button>
					) : (
						<Button onClick={handleSaveAndConnect} disabled={!isValid || connecting}>
							{connecting && <Loader2 className="mr-2 h-4 w-4 animate-spin" />}
							{t("connection.saveConnect")}
						</Button>
					)}
				</DialogFooter>
			</DialogContent>
		</Dialog>
	);
}
