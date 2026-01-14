import { useState, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
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

import { Loader2 } from "lucide-react";
import { toast } from "sonner";

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
  initialData,
  onSuccess
}: RowModalProps) {
  const { t } = useTranslation();
  const [loading, setLoading] = useState(false);
  const [formData, setFormData] = useState<Record<string, string>>({});
  const [nulls, setNulls] = useState<Record<string, boolean>>({});

  // Initialize form data
  useEffect(() => {
    if (isOpen) {
      const initialForm: Record<string, string> = {};
      const initialNulls: Record<string, boolean> = {};

      schema.columns.forEach(col => {
        let val = initialData?.[col.name];
        
        // If update mode and we have data
        if (mode === 'update' && val !== undefined) {
          if (val === null) {
            initialNulls[col.name] = true;
            initialForm[col.name] = '';
          } else {
            initialNulls[col.name] = false;
            initialForm[col.name] = String(val);
          }
        } else {
          // Insert mode or no data
          initialForm[col.name] = '';
          // Default to null if nullable and no default value
          if (col.nullable && !col.default_value) {
            initialNulls[col.name] = true;
          } else {
            initialNulls[col.name] = false;
          }
        }
      });
      
      setFormData(initialForm);
      setNulls(initialNulls);
    }
  }, [isOpen, schema, initialData, mode]);

  const handleInputChange = (col: string, value: string) => {
    setFormData(prev => ({ ...prev, [col]: value }));
    if (nulls[col]) {
      setNulls(prev => ({ ...prev, [col]: false }));
    }
  };

  const handleNullToggle = (col: string, isNull: boolean) => {
    setNulls(prev => ({ ...prev, [col]: isNull }));
  };

  const parseValue = (value: string, dataType: string): Value => {
    // Basic type inference/conversion
    const type = dataType.toLowerCase();
    if (type.includes('int') || type.includes('serial') || type.includes('float') || type.includes('double') || type.includes('numeric')) {
      if (value === '' || value === undefined) return null;
      return Number(value);
    }
    if (type.includes('bool')) {
      return value === 'true' || value === '1' || value === 'yes';
    }
    // JSON
    if (type.includes('json')) {
      try {
        return JSON.parse(value);
      } catch {
        return value; // specific error handling?
      }
    }
    return value;
  };

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setLoading(true);

    try {
      const data: TauriRowData = { columns: {} };
      
      schema.columns.forEach(col => {
        if (nulls[col.name]) {
          data.columns[col.name] = null;
        } else {
          const rawVal = formData[col.name];
          if (rawVal === '' && col.default_value) {
             return;
          }
          data.columns[col.name] = parseValue(rawVal, col.data_type);
        }
      });

      if (mode === 'insert') {
        const res = await insertRow(sessionId, namespace.database, namespace.schema, tableName, data);
        if (res.success) {
          toast.success(t('rowModal.insertSuccess'));
          onSuccess();
          onClose();
        } else {
          toast.error(res.error || t('rowModal.insertError'));
        }
      } else {
        // Update
        // Construct Primary Key
        const pkData: TauriRowData = { columns: {} };
        if (!schema.primary_key || schema.primary_key.length === 0) {
           throw new Error("No primary key found for update");
        }
        
        schema.primary_key.forEach(pk => {
          // Use initial data for PK components to identify the row
           let val = initialData?.[pk];
           pkData.columns[pk] = val ?? null;
        });

        // Filter out PK columns from data update to avoid "updating PK" issues if not intended,
        // though usually DBs allow updating other columns. 
        // Logic regarding skipping empty strings (default) applies here too.
        
        const res = await updateRow(sessionId, namespace.database, namespace.schema, tableName, pkData, data);
        if (res.success) {
          toast.success(t('rowModal.updateSuccess'));
          onSuccess();
          onClose();
        } else {
          toast.error(res.error || t('rowModal.updateError'));
        }
      }
    } catch (err) {
      console.error(err);
      toast.error(err instanceof Error ? err.message : 'Operation failed');
    } finally {
      setLoading(false);
    }
  };

  return (
    <Dialog open={isOpen} onOpenChange={onClose}>
      <DialogContent className="max-w-xl max-h-[90vh] overflow-y-auto">
        <DialogHeader>
          <DialogTitle>
            {mode === 'insert' ? t('rowModal.insertTitle') : t('rowModal.updateTitle', { table: tableName })}
          </DialogTitle>
        </DialogHeader>

        <form onSubmit={handleSubmit}>
          <div className="grid gap-4 py-4">
            {schema.columns.map(col => (
              <div key={col.name} className="grid gap-2">
                <div className="flex items-center justify-between">
                  <Label htmlFor={col.name} className="flex items-center gap-2">
                    {col.name}
                    <span className="text-xs text-muted-foreground font-mono font-normal">
                      ({col.data_type})
                    </span>
                    {col.is_primary_key && (
                      <span className="text-xs bg-yellow-100 text-yellow-800 px-1 rounded dark:bg-yellow-900 dark:text-yellow-100">PK</span>
                    )}
                  </Label>
                  
                  {col.nullable && (
                    <div className="flex items-center space-x-2">
                      <Checkbox 
                        id={`${col.name}-null`} 
                        checked={nulls[col.name] || false}
                        onCheckedChange={(checked) => handleNullToggle(col.name, checked as boolean)}
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
                  value={formData[col.name] || ''}
                  onChange={(e) => handleInputChange(col.name, e.target.value)}
                  disabled={nulls[col.name]}
                  placeholder={col.default_value ? `Default: ${col.default_value}` : ''}
                  className="font-mono text-sm"
                />
              </div>
            ))}
          </div>

          <DialogFooter>
            <Button type="button" variant="outline" onClick={onClose}>
              {t('common.cancel')}
            </Button>
            <Button type="submit" disabled={loading}>
              {loading && <Loader2 className="mr-2 h-4 w-4 animate-spin" />}
              {mode === 'insert' ? t('common.insert') : t('common.save')}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
