// SPDX-License-Identifier: Apache-2.0

import { useTranslation } from 'react-i18next';
import { type BlobKind, getDataUri } from '@/lib/binaryUtils';

interface ImagePreviewProps {
  base64: string;
  blobKind: BlobKind;
}

const KIND_LABEL: Record<BlobKind['kind'], string> = {
  image: '',
  svg: 'SVG',
};

export function ImagePreview({ base64, blobKind }: ImagePreviewProps) {
  const { t } = useTranslation();
  const dataUri = getDataUri(base64, blobKind.mime);
  const label = blobKind.kind === 'svg' ? KIND_LABEL.svg : blobKind.type.toUpperCase();

  return (
    <div className="p-4 flex flex-col items-center gap-2">
      <img
        src={dataUri}
        alt={t('blobViewer.imagePreview')}
        className="max-h-87.5 max-w-full object-contain rounded shadow-sm"
      />
      <span className="text-xs text-muted-foreground">{label}</span>
    </div>
  );
}
