import type {Message, UsageSummary} from '../../../../../packages/host-contract/src';

export function messageText(message: Message): string {
  return typeof message.content === 'string' ? message.content : Array.isArray(message.content)
    ? message.content.filter(block => block?.type === 'text').map(block => block.text ?? '').join('\n\n') : '';
}
export function durationLabel(ms: number): string {
  const seconds = Math.floor(Math.max(0, ms) / 1000);
  if (seconds < 60) return `${seconds}秒`;
  if (seconds < 3600) return `${Math.floor(seconds / 60)}分${seconds % 60}秒`;
  return `${Math.floor(seconds / 3600)}小时${Math.floor(seconds % 3600 / 60)}分${seconds % 60}秒`;
}
export function compactTokenLabel(value?: number): string {
  if (value === undefined) return '用量';
  const absolute = Math.abs(value);
  const units = [{threshold: 1e9, suffix: 'B'}, {threshold: 1e6, suffix: 'M'}, {threshold: 1e3, suffix: 'K'}];
  const unit = units.find(item => absolute >= item.threshold);
  if (!unit) return value.toLocaleString('zh-CN');
  const compact = (value / unit.threshold).toFixed(1).replace(/\.0$/, '');
  return `${compact}${unit.suffix}`;
}
const clockLabel = (date: Date) => date.toLocaleTimeString('zh-CN', {hour12: false});
const dateLabel = (date: Date) => [date.getFullYear(), String(date.getMonth() + 1).padStart(2, '0'), String(date.getDate()).padStart(2, '0')].join('-');

/** Use compact relative dates for recent turns and an unambiguous local date otherwise. */
export function completedAtLabel(timestamp: number, now = Date.now()): string {
  const date = new Date(timestamp);
  const today = new Date(now);
  today.setHours(0, 0, 0, 0);
  const day = new Date(date);
  day.setHours(0, 0, 0, 0);
  const daysAgo = Math.round((today.getTime() - day.getTime()) / 86_400_000);
  if (daysAgo === 0) return clockLabel(date);
  if (daysAgo === 1) return `昨日 ${clockLabel(date)}`;
  return `${dateLabel(date)} ${clockLabel(date)}`;
}

const finite = (value: unknown): number | undefined => typeof value === 'number' && Number.isFinite(value) && value >= 0 ? value : undefined;

/** Aggregate original model messages once, before splitting their thinking/text blocks. */
export function turnMetadata(messages: Message[], user?: Message) {
  const assistants = messages.filter(message => message.role === 'assistant');
  const usage: UsageSummary = {};
  const fields = {inputTokens:'input', outputTokens:'output', cacheReadTokens:'cacheRead', cacheWriteTokens:'cacheWrite', reasoningTokens:'reasoning', totalTokens:'totalTokens'} as const;
  for (const [field, rawField] of Object.entries(fields) as [keyof typeof fields, (typeof fields)[keyof typeof fields]][]) {
    const values = assistants.map(message => finite(message.usage?.[rawField] ?? message.usage?.[field]));
    // A partial sum must never look like a complete total.
    if (values.length && values.every(value => value !== undefined)) usage[field as keyof UsageSummary] = values.reduce<number>((sum, value) => sum + value!, 0);
  }
  const costs = assistants.map(message => finite(typeof message.usage?.cost === 'object' && message.usage.cost !== null ? message.usage.cost.total : message.usage?.cost));
  if (costs.length && costs.every(cost => cost !== undefined)) usage.cost = costs.reduce<number>((sum, cost) => sum + cost!, 0);
  if (!user) for (const key of Object.keys(usage)) delete usage[key as keyof UsageSummary];
  const last = assistants.at(-1);
  const startedAt = finite(user?.timestamp);
  const completedAt = finite(last?.completedAt);
  return {usage, startedAt, completedAt, durationMs: startedAt !== undefined && completedAt !== undefined && completedAt >= startedAt ? completedAt - startedAt : undefined,
    partial: !user || assistants.some(message => !message.usage), last};
}
