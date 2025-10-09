# SMTP Provider

This SMTP provider is based on [Actions-JS SMTP helper](https://gitlab.betty.services/code/actions-js/-/tree/edge/src/worker/helpers/smtp?ref_type=heads) and [Chiel's SMTP function](https://github.com/Betty-Blocks-Services/chiels-block-store-functions/blob/main/functions/send-email/2.3/index.js).

## Running Locally

First, you can create a temporary mail catcher with [Ethereal Email](https://ethereal.email/messages).
When you created a temporary account, you should update the credentials in `send.sh`.

1. Build the provider and component:
`wash build`
`cd ../../components/smtp/; wash wit deps; wash build; cd -`

2. Deploy the app:
`wash app deploy wadm.yaml`
or
`wash app deploy wadm.yaml --replace`

3. Execute `./send.sh` to send an email

4. Check `ethereal.email/messages` whether emailing was successfull.
