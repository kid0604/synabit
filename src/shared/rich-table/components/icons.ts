import type { Component } from 'vue';
import {
  Type, Hash, CircleChevronDown, Tags, Calendar, SquareCheck, Link, FileText, Sigma,
} from 'lucide-vue-next';
import type { ColumnType } from '../model';

export const TYPE_ICONS: Record<ColumnType, Component> = {
  text: Type,
  number: Hash,
  select: CircleChevronDown,
  multi: Tags,
  date: Calendar,
  checkbox: SquareCheck,
  url: Link,
  note: FileText,
  formula: Sigma,
};
