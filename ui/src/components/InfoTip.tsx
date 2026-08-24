import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";

/**
 * The two sentences the local fields used to carry in their labels. Both the
 * card row and the form field read them from here, so the label and the form
 * cannot say two different things about the same field.
 */
export const NICKNAME_INFO = "Your local nickname for this identity. Only this device sees it.";
export const NOTE_INFO = "A private note about this identity. Only this device sees it.";

function InfoIcon() {
  return (
    <svg viewBox="0 0 16 16" aria-hidden="true" className="size-3.5" fill="currentColor">
      <path d="M8 1.5a6.5 6.5 0 1 0 0 13 6.5 6.5 0 0 0 0-13ZM8 3a1 1 0 1 1 0 2 1 1 0 0 1 0-2Zm.75 3.75v5h-1.5v-5h1.5Z" />
    </svg>
  );
}

/**
 * The one thing this app does with a label it will not lengthen: a small info
 * icon beside it, holding the sentence the label used to carry. It opens on
 * hover, on focus and on a tap, because a phone has no hover.
 */
export function InfoTip({ text, testId }: { text: string; testId: string }) {
  return (
    <Tooltip className="align-middle">
      <TooltipTrigger data-testid={testId} aria-label={text} className="size-5 shrink-0">
        <InfoIcon />
      </TooltipTrigger>
      <TooltipContent data-testid={`${testId}-text`}>{text}</TooltipContent>
    </Tooltip>
  );
}
