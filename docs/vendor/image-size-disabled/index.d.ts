// Types for the image-size stand-in. See fromFile.cjs for the rationale (T108).

export type {ISizeCalculationResult} from './fromFile';
import type {ISizeCalculationResult} from './fromFile';

export declare function imageSize(input: Uint8Array): ISizeCalculationResult;
export declare function imageSizeFromFile(filePath: string): Promise<ISizeCalculationResult>;
export declare function setConcurrency(limit: number): void;
export declare function disableTypes(types: readonly string[]): void;
export declare const types: readonly string[];
export default imageSize;
