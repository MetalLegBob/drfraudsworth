export interface TutorialStepCopy {
  title: string;
  description: string;
  warning?: string;
}

export interface TutorialStep extends TutorialStepCopy {
  selector: string;
}

export const TUTORIAL_INTRO_TITLE = 'Welcome to the Factory!';
export const TUTORIAL_INTRO_SUBTITLE =
  'A multilayered system where fees fund rewards and randomness reshapes the market.';

const STEP_COPY = {
  wallet: {
    title: 'Connect Your Wallet',
    description: 'Link your wallet to enter the Factory and start trading.',
  },
  swap: {
    title: 'Swap Station',
    description:
      'Buy and sell tokens inside the system. All trading happens here, and most importantly, access to $PROFIT.',
    warning: 'Tokens can not be transferred to other wallets due to custom AMMs.',
  },
  carnage: {
    title: 'Carnage Cauldron',
    description:
      'The risk engine. A portion of all trading fees builds the Carnage Fund. At random times, it triggers - buying tokens from the market and burning them. Result: sudden price moves, supply reduction, and volatility you can play.',
  },
  staking: {
    title: 'Rewards',
    description:
      "Stake $PROFIT to earn real SOL rewards. Rewards come from all trading activity as every transaction has to go through the custom AMM's and they charge a tax that benefits stakers.",
  },
  docs: {
    title: 'Full Docs',
    description:
      'See the full system explained: Tax flips each epoch (randomised), arbitrage drives volume, and volume funds rewards + Carnage. If something feels chaotic - that is intentional.',
  },
  settings: {
    title: 'Settings Console',
    description: 'Control slippage, priority fees, wallet connection.',
  },
} as const satisfies Record<string, TutorialStepCopy>;

export const DESKTOP_TUTORIAL_STEPS: TutorialStep[] = [
  {
    ...STEP_COPY.wallet,
    selector: '[data-tutorial-id="station-wallet"]',
  },
  {
    ...STEP_COPY.swap,
    selector: '[data-tutorial-id="station-swap"]',
  },
  {
    ...STEP_COPY.carnage,
    selector: '[data-tutorial-id="station-carnage"]',
  },
  {
    ...STEP_COPY.staking,
    selector: '[data-tutorial-id="station-staking"]',
  },
  {
    ...STEP_COPY.docs,
    selector: '[data-tutorial-id="station-docs"]',
  },
  {
    ...STEP_COPY.settings,
    selector: '[data-tutorial-id="station-settings"]',
  },
];

export const MOBILE_TUTORIAL_STEPS: TutorialStep[] = [
  {
    ...STEP_COPY.wallet,
    title: 'Wallet Access',
    description: 'Tap this row to connect and manage your wallet.',
    selector: '[data-tutorial-id="mobile-station-wallet"]',
  },
  {
    ...STEP_COPY.swap,
    selector: '[data-tutorial-id="mobile-station-swap"]',
  },
  {
    ...STEP_COPY.staking,
    selector: '[data-tutorial-id="mobile-station-staking"]',
  },
  {
    ...STEP_COPY.carnage,
    description:
      'The risk engine. A portion of all trading fees builds the Carnage Fund. At random times, it triggers - buying tokens from the market and burning them. Result: sudden price moves, supply reduction, and volatility you can play.',
    selector: '[data-tutorial-id="mobile-station-carnage"]',
  },
  {
    ...STEP_COPY.docs,
    selector: '[data-tutorial-id="mobile-station-docs"]',
  },
  {
    ...STEP_COPY.settings,
    title: 'Settings',
    selector: '[data-tutorial-id="mobile-station-settings"]',
  },
];
