import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";

/** The usual `cn` — clsx for conditionals, twMerge so later classes win. */
export function cn(...inputs: ClassValue[]): string {
  return twMerge(clsx(inputs));
}
