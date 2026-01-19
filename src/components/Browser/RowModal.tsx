import { useState, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { Loader2 } from 'lucide-react';
import { notify } from '../../lib/notify';

import { 
  TableSchema,
  Value,
  insertRow,
  updateRow,
  Namespace,
  TableColumn,
  RowData as TauriRowData
} from '../../lib/tauri';
import { Driver } from '../../lib/drivers';
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
} from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { RowModalCustomFields } from './RowModalCustomFields';
import { RowModalSchemaFields } from './RowModalSchemaFields';
import { RowModalExtraFields } from './RowModalExtraFields';
import { RowModalUpdatePreview } from './RowModalUpdatePreview';
import {
  buildColumnsData,
  buildInitialRowModalState,
  computePreview,
  formatPreviewValue,
} from './rowModalUtils';

interface RowModalProps {
  isOpen: boolean;
  onClose: () => void;
  mode: 'insert' | 'update';
  sessionId: string;
  namespace: Namespace;
  tableName: string;
  schema: TableSchema;
  driver?: Driver;

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
  driver,
  readOnly = false,
  initialData,
  onSuccess
}: RowModalProps) {
	const { t } = useTranslation();
	const [loading, setLoading] = useState(false);
	const [formData, setFormData] = useState<Record<string, string>>({});
	const [nulls, setNulls] = useState<Record<string, boolean>>({});
  const [previewError, setPreviewError] = useState<string | null>(null);

  // Dynamic fields for NoSQL
  const [extraColumns, setExtraColumns] = useState<TableColumn[]>([]);
  const [newFieldName, setNewFieldName] = useState("");
  const [newFieldType, setNewFieldType] = useState("string");

  const effectiveColumns = [...schema.columns, ...extraColumns];

  // Initialize form data
  useEffect(() => {
    if (isOpen) {
      const { formData, nulls, extraColumns } = buildInitialRowModalState({
        schema,
        initialData,
        mode,
        driver,
      });

      setFormData(formData);
      setNulls(nulls);
      setExtraColumns(extraColumns);
      setPreviewError(null);
      setNewFieldName("");
      setNewFieldType("string");
    }
  }, [isOpen, schema, initialData, mode, driver]);

  const handleAddExtraField = () => {
    if (!newFieldName.trim()) return;
    if (effectiveColumns.find(c => c.name === newFieldName)) {
      notify.error(t("rowModal.fieldExists"));
      return;
    }

    const newCol: TableColumn = {
      name: newFieldName,
      data_type: newFieldType,
      nullable: true,
      is_primary_key: false
    };

    setExtraColumns([...extraColumns, newCol]);
    setFormData(prev => ({ ...prev, [newFieldName]: "" }));
    setNulls(prev => ({ ...prev, [newFieldName]: false }));
    setNewFieldName("");
  };

  const handleRemoveExtraField = (colName: string) => {
    setExtraColumns(prev => prev.filter(c => c.name !== colName));
    setFormData(prev => {
      const next = { ...prev };
      delete next[colName];
      return next;
    });
    setNulls(prev => {
      const next = { ...prev };
      delete next[colName];
      return next;
    });
  };

	const handleInputChange = (col: string, value: string) => {
		setFormData((prev) => ({ ...prev, [col]: value }));
		if (nulls[col]) {
			setNulls((prev) => ({ ...prev, [col]: false }));
		}
	};

	const handleNullToggle = (col: string, isNull: boolean) => {
		setNulls((prev) => ({ ...prev, [col]: isNull }));
	};

	const handleSubmit = async (e: React.FormEvent) => {
		e.preventDefault();
		if (readOnly) {
			notify.error(t("environment.blocked"));
			return;
		}
		setPreviewError(null);
		setLoading(true);

		try {
      const data: TauriRowData = {
        columns: buildColumnsData({
          columns: effectiveColumns,
          formData,
          nulls,
        }),
      };

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
					notify.success(t("rowModal.insertSuccess") + timeMsg);
					onSuccess();
					onClose();
				} else {
					notify.error(t("rowModal.insertError"), res.error);
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
					notify.success(t("rowModal.updateSuccess") + timeMsg);
					onSuccess();
					onClose();
				} else {
					notify.error(t("rowModal.updateError"), res.error);
				}
			}
		} catch (err) {
			console.error(err);
			const message = err instanceof Error ? err.message : "Operation failed";
			setPreviewError(message);
			notify.error(message, err);
		} finally {
			setLoading(false);
		}
	};

	const preview = computePreview({
    mode,
    schema,
    initialData,
    effectiveColumns,
    formData,
    nulls,
  });
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
            {mode === 'insert'
              ? t('rowModal.insertTitle')
              : t('rowModal.updateTitle', { table: tableName })}
          </DialogTitle>
        </DialogHeader>

        <form onSubmit={handleSubmit}>
          <RowModalSchemaFields
            columns={schema.columns}
            formData={formData}
            nulls={nulls}
            readOnly={readOnly}
            onNullToggle={handleNullToggle}
            onInputChange={handleInputChange}
          />

          {driver === Driver.Mongodb && (
            <RowModalCustomFields
              title={t('rowModal.addCustomField')}
              fieldNameLabel={t('rowModal.fieldName')}
              fieldTypeLabel={t('rowModal.fieldType')}
              addLabel={t('common.add')}
              fieldNamePlaceholder={t('fieldNamePlaceholder')}
              newFieldName={newFieldName}
              newFieldType={newFieldType}
              readOnly={readOnly}
              onNameChange={setNewFieldName}
              onTypeChange={setNewFieldType}
              onAdd={handleAddExtraField}
            />
          )}

          <RowModalExtraFields
            columns={extraColumns}
            formData={formData}
            nulls={nulls}
            readOnly={readOnly}
            title={t('rowModal.customFields')}
            onNullToggle={handleNullToggle}
            onInputChange={handleInputChange}
            onRemove={handleRemoveExtraField}
          />

          {mode === 'update' && (
            <RowModalUpdatePreview
              changes={updatePreview?.changes ?? []}
              isEmpty={previewIsEmpty}
              error={previewError}
              title={t('rowModal.previewTitle')}
              emptyLabel={t('rowModal.previewEmpty')}
              formatValue={formatPreviewValue}
            />
          )}

          <DialogFooter>
            <Button type="button" variant="outline" onClick={onClose}>
              {t('common.cancel')}
            </Button>
            <Button
              type="submit"
              disabled={loading || readOnly || !hasPreviewChanges}
              title={readOnly ? t('environment.blocked') : undefined}
            >
              {loading && <Loader2 className="mr-2 h-4 w-4 animate-spin" />}
              {mode === 'insert' ? t('common.insert') : t('common.save')}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
