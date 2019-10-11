// Copyright (c) 2017-2019, Substratum LLC (https://substratum.net) and/or its affiliates. All rights reserved.

import {async, ComponentFixture, TestBed} from '@angular/core/testing';
import {IndexComponent} from './index.component';
import {FooterComponent} from '../footer/footer.component';
import * as td from 'testdouble';
import {MainService} from '../main.service';
import {BehaviorSubject} from 'rxjs';
import {NodeStatus} from '../node-status.enum';
import {FormsModule, ReactiveFormsModule} from '@angular/forms';
import {ConfigService} from '../config.service';
import {Component, ElementRef, Input, ViewChild} from '@angular/core';
import {ConfigurationMode} from '../configuration-mode.enum';
import {ConsumingWalletPasswordPromptComponent} from '../consuming-wallet-password-prompt/consuming-wallet-password-prompt.component';
import {TabsComponent} from '../tabs/tabs.component';
import {TabComponent} from '../tabs/tab.component';
import {NodeConfiguration, NodeConfigurations} from '../node-configuration';
import {LocalStorageService} from '../local-storage.service';
import {LocalServiceKey} from '../local-service-key.enum';
import {Network} from 'vis-network';
import {RouterTestingModule} from '@angular/router/testing';

@Component({selector: 'app-node-configuration', template: '<div id="node-config"></div>'})
class NodeConfigurationStubComponent {
  @Input() mode: ConfigurationMode;
  @Input() status: NodeStatus;
}

@Component({selector: 'app-header', template: ''})
class HeaderStubComponent {
}

@Component({selector: 'app-status', template: ''})
class StatusStubComponent {
}

@Component({selector: 'app-financial-statistics', template: ''})
class FinancialStatisticsStubComponent {
  @Input() status: NodeStatus;
  @Input() tokenSymbol: string;
}

@Component({selector: 'app-neighborhood', template: ''})
class NeighborhoodStubComponent {
  @Input() status: NodeStatus;
  @ViewChild('useIcon', {static: false}) useIcon: ElementRef;
  @ViewChild('neighborhoodData', {static: false}) neighborhoodData: ElementRef;
  neighborhood: Network;
  dotGraph: string;
}

describe('IndexComponent', () => {
  let component: IndexComponent;
  let fixture: ComponentFixture<IndexComponent>;
  let compiled;
  let mockMainService;
  let mockConfigService;
  let mockLocalStorageService;
  let mockStatus: BehaviorSubject<NodeStatus>;
  let mockNodeDescriptor: BehaviorSubject<string>;
  let mockSetWalletPasswordResponse: BehaviorSubject<boolean>;
  let offButton;
  let servingButton;
  let consumingButton;
  let mockMode;
  let storedConfigs: BehaviorSubject<NodeConfigurations>;
  let storedLookupIp: BehaviorSubject<string>;
  let mockChainName: BehaviorSubject<string>;

  beforeEach(async(() => {
    mockStatus = new BehaviorSubject(NodeStatus.Off);
    mockMode = new BehaviorSubject(ConfigurationMode.Hidden);
    mockNodeDescriptor = new BehaviorSubject('');
    mockSetWalletPasswordResponse = new BehaviorSubject(false);
    storedConfigs = new BehaviorSubject(null);
    storedLookupIp = new BehaviorSubject('192.168.1.1');
    mockChainName = new BehaviorSubject('ropsten');
    mockMainService = {
      turnOff: td.func('turnOff'),
      serve: td.func('serve'),
      consume: td.func('consume'),
      copyToClipboard: td.func(),
      setConsumingWalletPassword: td.func(),
      nodeStatus: mockStatus.asObservable(),
      nodeDescriptor: mockNodeDescriptor.asObservable(),
      setConsumingWalletPasswordResponse: mockSetWalletPasswordResponse.asObservable(),
      lookupIp: td.func('lookupIp'),
      chainName: mockChainName.asObservable(),
      chainNameListener: mockChainName,
    };
    spyOn(mockMainService, 'copyToClipboard');
    spyOn(mockMainService, 'setConsumingWalletPassword');
    mockConfigService = {
      getConfigs: td.func('getConfigs'),
      isValidServing: td.func('isValidServing'),
      isValidConsuming: td.func('isValidConsuming'),
      mode: mockMode,
      load: td.func('load'),
      patchValue: td.func(),
      setMode: td.func('setMode'),
    };
    spyOn(mockConfigService, 'patchValue');
    mockLocalStorageService = {
      getItem: td.func('getItem'),
      setItem: td.func('setItem'),
      removeItem: td.func('removeItem')
    };
    return TestBed.configureTestingModule({
      declarations: [
        IndexComponent,
        StatusStubComponent,
        HeaderStubComponent,
        NodeConfigurationStubComponent,
        ConsumingWalletPasswordPromptComponent,
        FinancialStatisticsStubComponent,
        NeighborhoodStubComponent,
        TabsComponent,
        TabComponent,
        FooterComponent
      ],
      imports: [
        RouterTestingModule.withRoutes([]),
        FormsModule,
        ReactiveFormsModule
      ],
      providers: [
        {provide: MainService, useValue: mockMainService},
        {provide: ConfigService, useValue: mockConfigService},
        {provide: LocalStorageService, useValue: mockLocalStorageService},
      ]
    }).compileComponents();
  }));

  afterEach(() => {
    td.reset();
  });

  beforeEach(() => {
    td.when(mockMainService.lookupIp()).thenReturn(storedLookupIp.asObservable());
    td.when(mockConfigService.load()).thenReturn(storedConfigs.asObservable());
    fixture = TestBed.createComponent(IndexComponent);
    component = fixture.componentInstance;
    compiled = fixture.debugElement.nativeElement;
    offButton = compiled.querySelector('#off');
    servingButton = compiled.querySelector('#serving');
    consumingButton = compiled.querySelector('#consuming');

    td.when(mockMainService.serve()).thenDo(() => mockStatus.next(NodeStatus.Serving));
    td.when(mockMainService.consume()).thenDo(() => mockStatus.next(NodeStatus.Consuming));
    td.when(mockMainService.turnOff()).thenDo(() => mockStatus.next(NodeStatus.Off));
    fixture.detectChanges();
  });

  describe('LookupIp', () => {
    describe('successful ip address lookup', () => {
      describe('ip is filled out if it can be looked up', () => {
        beforeEach(()  => {
          storedConfigs.next({ropsten: {ip: '192.168.1.1'}});
          mockChainName.next('ropsten');
          fixture.detectChanges();
        });

        it('ip address is filled out', () => {
          expect(mockConfigService.patchValue).toHaveBeenCalledWith({ropsten: {ip: '192.168.1.1'}});
        });
      });
    });

    describe('unsuccessful ip address lookup', () => {
      beforeEach(() => {
        storedConfigs.next({ropsten: {ip: ''}});
        storedLookupIp.next('');
        fixture.detectChanges();
      });

      describe('the ip field', () => {
        it('ip address starts blank', () => {
          expect(mockConfigService.patchValue).toHaveBeenCalledWith({ropsten: {ip: ''}});
        });
      });
    });
  });

  describe('When configuration is loaded', () => {
    describe('and values are not in the configuration but are in local storage', () => {
      beforeEach(() => {
        mockChainName.next('ropsten');
        td.when(mockLocalStorageService.getItem(`ropsten.${LocalServiceKey.NeighborNodeDescriptor}`))
          .thenReturn('5sqcWoSuwaJaSnKHZbfKOmkojs0IgDez5IeVsDk9wno:2.2.2.2:1999');
        td.when(mockLocalStorageService.getItem(`ropsten.${LocalServiceKey.BlockchainServiceUrl}`)).thenReturn('https://ropsten.infura.io');

        const nextConfigs = new NodeConfigurations();
        nextConfigs.ropsten = {
          neighbor: '',
          blockchainServiceUrl: '',
        } as NodeConfiguration;
        storedConfigs.next(nextConfigs);
        component.loadLocalStorage();
        fixture.detectChanges();
      });

      it('ConfigService is patched with data from local storage', () => {
        const expected = new NodeConfigurations();
        expected.ropsten = {
          neighbor: '5sqcWoSuwaJaSnKHZbfKOmkojs0IgDez5IeVsDk9wno:2.2.2.2:1999',
          blockchainServiceUrl: 'https://ropsten.infura.io',
        };
        expect(mockConfigService.patchValue).toHaveBeenCalledWith(expected);
      });
    });

    describe('and values are in the configuration but not in local storage', () => {
      beforeEach(() => {
        td.when(mockLocalStorageService.getItem(`ropsten.${LocalServiceKey.NeighborNodeDescriptor}`)).thenReturn('');
        td.when(mockLocalStorageService.getItem(`ropsten.${LocalServiceKey.BlockchainServiceUrl}`)).thenReturn('');
        storedConfigs.next({ropsten: {
          neighbor: '5sqcWoSuwaJaSnKHZbfKOmkojs0IgDez5IeVsDk9wno:2.2.2.2:1999',
          blockchainServiceUrl: 'https://ropsten.infura.io',
        } as NodeConfiguration} as NodeConfigurations);
        component.loadLocalStorage();
        fixture.detectChanges();
      });

      it('ConfigService is patched with data from config parameter', () => {
        expect(mockConfigService.patchValue).toHaveBeenCalledWith({ropsten: {
          neighbor: '5sqcWoSuwaJaSnKHZbfKOmkojs0IgDez5IeVsDk9wno:2.2.2.2:1999',
          blockchainServiceUrl: 'https://ropsten.infura.io',
        }});
      });
    });
  });
});
