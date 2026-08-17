// Types for the image-size stand-in. Shapes match the real package so that a type-aware
// consumer compiles identically; the runtime always throws. See fromFile.cjs (T108).

export interface ISizeCalculationResult {
  width?: number;
  height?: number;
  type?: string;
  orientation?: number;
  images?: ISizeCalculationResult[];
}

/** Always rejects — this project ships no images and no image parser. */
export declare function imageSizeFromFile(filePath: string): Promise<ISizeCalculationResult>;

/** No-op; retained for signature compatibility with the real package. */
export declare function setConcurrency(limit: number): void;
