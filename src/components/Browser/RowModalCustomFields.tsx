import { Plus } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '../ui/label';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';

interface RowModalCustomFieldsProps {
  title: string;
  fieldNameLabel: string;
  fieldTypeLabel: string;
  addLabel: string;
  fieldNamePlaceholder: string;
  newFieldName: string;
  newFieldType: string;
  readOnly: boolean;
  onNameChange: (value: string) => void;
  onTypeChange: (value: string) => void;
  onAdd: () => void;
}

export function RowModalCustomFields({
  title,
  fieldNameLabel,
  fieldTypeLabel,
  addLabel,
  fieldNamePlaceholder,
  newFieldName,
  newFieldType,
  readOnly,
  onNameChange,
  onTypeChange,
  onAdd,
}: RowModalCustomFieldsProps) {
  return (
    <div className="border border-dashed border-border rounded-md p-4 mb-4">
      <div className="text-sm font-medium mb-3 flex items-center gap-2">
        <Plus size={16} />
        {title}
      </div>
      <div className="flex items-end gap-2">
        <div className="grid gap-1.5 flex-1">
          <Label htmlFor="new-field-name">{fieldNameLabel}</Label>
          <Input
            id="new-field-name"
            value={newFieldName}
            onChange={e => onNameChange(e.target.value)}
            placeholder={fieldNamePlaceholder}
            className="h-8"
          />
        </div>
        <div className="grid gap-1.5 w-32">
          <Label htmlFor="new-field-type">{fieldTypeLabel}</Label>
          <Select value={newFieldType} onValueChange={onTypeChange}>
            <SelectTrigger id="new-field-type" className="h-8">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="string">String</SelectItem>
              <SelectItem value="double">Number</SelectItem>
              <SelectItem value="boolean">Boolean</SelectItem>
              <SelectItem value="json">JSON</SelectItem>
              <SelectItem value="datetime">Date</SelectItem>
            </SelectContent>
          </Select>
        </div>
        <Button type="button" size="sm" onClick={onAdd} disabled={!newFieldName.trim() || readOnly}>
          {addLabel}
        </Button>
      </div>
    </div>
  );
}
