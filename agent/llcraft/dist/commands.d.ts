/**
 * Command handlers for llcraft REPL
 */
import { Session, Config, CommandResult } from './types.js';
export declare function executeCommand(input: string, session: Session, config: Config): CommandResult;
