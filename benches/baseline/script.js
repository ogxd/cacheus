import http from 'k6/http';

export const options = {
  discardResponseBodies: true,
  cloud: {
    projectID: 4565599,
    name: 'Cacheus to Hello World',
  },
  scenarios: {
    contacts: {
      executor: 'ramping-arrival-rate',
      // Start iterations per `timeUnit`
      startRate: 100,
      // Start `startRate` iterations per minute
      timeUnit: '1s',
      // Pre-allocate necessary VUs.
      preAllocatedVUs: 10000,
      stages: [
        { target: 20000, duration: '5m' },
      ],
    },
  },
};

export default function() {
  http.get('http://127.0.0.1:8080');
}